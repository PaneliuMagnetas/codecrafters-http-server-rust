use std::error::Error;
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use nom::{branch::alt, bytes::complete::*, multi::*, IResult};

#[allow(dead_code)]
struct Request {
    method: String,
    path: String,
    version: String,
    headers: Vec<Header>,
    content: Vec<u8>,
}

#[derive(Debug)]
struct Header {
    name: String,
    value: String,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").await?;

    let mut args = std::env::args();
    let mut directory = None;

    if let Some(arg) = args.nth(1) {
        if arg == "--directory" {
            directory = Some(args.nth(0).unwrap());
        }
    }

    loop {
        let (mut socket, _) = listener.accept().await?;

        let directory = directory.clone();
        tokio::spawn(async move {
            write_response(&mut socket, directory).await;
        });
    }
}

async fn write(socket: &mut TcpStream, text: &str) {
    let _ = socket.write(text.as_bytes()).await;
}

async fn write_response(socket: &mut TcpStream, directory: Option<String>) {
    let request = match read_request(socket).await {
        Ok(request) => request,
        Err(_) => {
            return;
        }
    };

    match request.path.as_str() {
        "/" => {
            write(socket, "HTTP/1.1 200 OK\r\n\r\n").await;
        }
        "/user-agent" => {
            let mut response =
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 0\r\n\r\n"
                    .to_string();

            for header in request.headers {
                if header.name == "User-Agent" {
                    response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        header.value.len(),
                        header.value,
                    );
                    break;
                }
            }

            write(socket, &response).await;
        }
        s if s.starts_with("/echo/") => {
            let mut split = s.splitn(2, "/echo/");

            let message = match split.nth(1) {
                Some(message) => message,
                None => {
                    write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                    return;
                }
            };

            write(
                socket,
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                    message.len(),
                    message,
                )
                .as_str(),
            )
            .await;
        }
        s if s.starts_with("/files/") => {
            let mut split = s.splitn(2, "/files/");

            let directory = match directory {
                Some(directory) => directory,
                None => {
                    write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                    return;
                }
            };

            let file = match split.nth(1) {
                Some(file) => file,
                None => {
                    write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                    return;
                }
            };

            let path = format!("{}/{}", directory, file);

            handle_files(socket, request, path.as_str()).await;
        }
        _ => {
            write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
        }
    };
}

async fn handle_files(socket: &mut TcpStream, request: Request, file_path: &str) {
    match request.method.as_str() {
        "GET" => {
            let content_length = match tokio::fs::metadata(file_path).await {
                Ok(metadata) => metadata.len(),
                Err(_) => {
                    write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                    return;
                }
            };

            let mut file = match tokio::fs::File::open(file_path).await {
                Ok(file) => file,
                Err(_) => {
                    write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                    return;
                }
            };

            write(socket, format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n", content_length).as_str()).await;

            let mut buffer = [0; 1024];
            loop {
                match file.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = socket.write(&buffer[0..n]).await;
                    }
                    Err(_) => {
                        write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                        return;
                    }
                }
            }
        }
        "POST" => {
            let mut file = match tokio::fs::File::create(file_path).await {
                Ok(file) => file,
                Err(_) => {
                    write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
                    return;
                }
            };

            let _ = file.write(&request.content).await;

            let mut buffer = [0; 1024];
            loop {
                match socket.try_read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = file.write(&buffer[0..n]).await;
                    }
                    Err(_) => (),
                }
            }

            write(socket, "HTTP/1.1 201 CREATED\r\n\r\n").await;
        }
        _ => {
            write(socket, "HTTP/1.1 404 NOT FOUND\r\n\r\n").await;
        }
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<Request, Box<dyn Error>> {
    let mut buffer = [0; 1024];
    let _ = stream.read(&mut buffer).await;

    let request = match parse_request(&buffer) {
        Ok((_, request)) => request,
        Err(e) => {
            return Err(Box::new(e.to_owned()));
        }
    };

    Ok(request)
}

fn parse_request(input: &[u8]) -> IResult<&[u8], Request> {
    let (input, method) = method(input)?;
    let (input, _) = space(input)?;
    let (input, path) = path(input)?;
    let (input, _) = space(input)?;
    let (input, version) = version(input)?;
    let (input, _) = crlf(input)?;
    let (input, headers) = match headers(input) {
        Ok((input, headers)) => (input, headers),
        Err(_) => (input, vec![]),
    };

    let (input, _) = crlf(input)?;

    Ok((
        input,
        Request {
            method: from_utf8(method).unwrap(),
            path: from_utf8(path).unwrap(),
            version: from_utf8(version).unwrap(),
            headers,
            content: input.to_vec(),
        },
    ))
}

fn space(input: &[u8]) -> IResult<&[u8], &[u8]> {
    tag(" ")(input)
}

fn method(input: &[u8]) -> IResult<&[u8], &[u8]> {
    alt((tag("GET"), tag("POST")))(input)
}

fn path(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while(|c| c != b' ')(input)
}

fn version(input: &[u8]) -> IResult<&[u8], &[u8]> {
    tag("HTTP/1.1")(input)
}

fn crlf(input: &[u8]) -> IResult<&[u8], &[u8]> {
    tag("\r\n")(input)
}

fn headers(input: &[u8]) -> IResult<&[u8], Vec<Header>> {
    many1(header)(input)
}

fn header(input: &[u8]) -> IResult<&[u8], Header> {
    let (input, name) = take_while(|c| c != b':')(input)?;
    let (input, _) = tag(": ")(input)?;
    let (input, value) = take_while(|c| c != b'\r')(input)?;
    let (input, _) = crlf(input)?;

    Ok((
        input,
        Header {
            name: from_utf8(name).unwrap(),
            value: from_utf8(value).unwrap(),
        },
    ))
}

fn from_utf8(input: &[u8]) -> Result<String, Box<dyn Error>> {
    let s = std::str::from_utf8(input)?;

    Ok(s.to_string())
}
