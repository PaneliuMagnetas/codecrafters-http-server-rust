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
}

#[derive(Debug)]
struct Header {
    name: String,
    value: String,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").await?;

    loop {
        let (socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            process_socket(socket).await;
        });
    }
}

async fn process_socket(mut stream: TcpStream) {
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(_) => {
            return;
        }
    };

    let _ = stream
        .write(generate_response(request).await.as_bytes())
        .await;
}

async fn generate_response(request: Request) -> String {
    match request.path.as_str() {
        "/" => "HTTP/1.1 200 OK\r\n\r\n".to_string(),
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

            return response;
        }
        s if s.starts_with("/echo/") => {
            let mut split = s.splitn(2, "/echo/");

            let message = match split.nth(1) {
                Some(message) => message,
                None => {
                    return "HTTP/1.1 404 NOT FOUND\r\n\r\n".to_string();
                }
            };

            return format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                message.len(),
                message,
            );
        }
        _ => "HTTP/1.1 404 NOT FOUND\r\n\r\n".to_string(),
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<Request, Box<dyn Error>> {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).await?;

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
