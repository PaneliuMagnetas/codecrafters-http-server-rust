use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use nom::{branch::alt, bytes::complete::*, multi::*, IResult};

#[derive(Debug)]
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

#[derive(Debug)]
enum Error {
    ParseError,
    IOError,
    InvalidUTF8,
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut _stream) => {
                println!("accepted new connection");

                let request = match read_request(&mut _stream) {
                    Ok(request) => request,
                    Err(e) => {
                        continue;
                    }
                };

                if request.path != "/" {
                    _stream
                        .write("HTTP/1.1 404 NOT FOUND\r\n\r\n".as_bytes())
                        .unwrap();
                    continue;
                }

                _stream.write("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).unwrap();
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Request, Error> {
    let mut buffer = [0; 1024];
    if let Err(e) = stream.read(&mut buffer) {
        println!("error: {}", e);
        return Err(Error::IOError);
    }

    let request = match parse_request(&buffer) {
        Ok((_, request)) => request,
        Err(e) => {
            println!("error: {}", e);
            return Err(Error::ParseError);
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
    let (input, headers) = headers(input)?;
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

fn from_utf8(input: &[u8]) -> Result<String, Error> {
    let s = std::str::from_utf8(input).map_err(|_| Error::InvalidUTF8)?;

    Ok(s.to_string())
}
