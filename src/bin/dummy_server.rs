use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 5000))?;
    println!("Dummy Basic Auth server listening on http://127.0.0.1:5000");

    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream) {
                    eprintln!("Request failed: {error}");
                }
            }
            Err(error) => eprintln!("Connection failed: {error}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    if request_line.is_empty() {
        return Ok(());
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");

    let mut authorization = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_string());
            }
        }
    }

    let credentials = authorization
        .as_deref()
        .and_then(|header| header.strip_prefix("Basic "))
        .and_then(decode_base64)
        .and_then(|bytes| String::from_utf8(bytes).ok());
    let accepted = credentials.as_deref().is_some_and(|credentials| {
        credentials
            .split_once(':')
            .is_some_and(|(username, password)| !username.is_empty() && !password.is_empty())
    });

    let (status, body) = if path == "/logout" || (path == "/" && !accepted) {
        ("200 OK", LOGIN_XML)
    } else if path == "/" && accepted {
        ("200 OK", WELCOME_XML)
    } else {
        ("404 Not Found", "<text value=\"Not found\"/>")
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    reader.get_mut().write_all(response.as_bytes())
}

const LOGIN_XML: &str = r#"
<title>Login</title>
<div style="padding: 1; width: 40; height: 20; border: true; border_color: cyan;">
    <text value="Welcome back" style="bold: true; align: center;" />
    <text value="Login" style="align: center;" />
    <form onsubmit="(() => { const username = getInputValue('username').trim(); const password = getInputValue('password'); if (!username || !password.trim()) return 'Please fill in both fields.'; const headers = 'Authorization: Basic ' + base64Encode(username + ':' + password); return httpRequest('GET', 'http://127.0.0.1:5000/', headers, ''); })()">
        <divider label="YOUR DETAILS" style="border-color: darkgray;" />
        <input id="username" label="Username" value="" style="width: 30;" />
        <input id="password" label="Password" type="password" value="" style="width: 30;" />
        <divider style="border-color: darkgray;" />
        <button label="Sign in" style="align: center;" />
    </form>
</div>
"#;

const WELCOME_XML: &str = r#"
<title>Welcome</title>
<div style="padding: 1; width: 40; height: 11; border: true; border_color: green;">
    <text value="You are signed in!" style="bold: true; align: center;" />
    <divider label="WELCOME" style="border-color: green;" />
    <text value="Your credentials were accepted." style="align: center;" />
    <form onsubmit="httpRequest('GET', 'http://127.0.0.1:5000/logout', '', '')">
        <button label="Log out" style="align: center;" />
    </form>
</div>
"#;

fn decode_base64(encoded: &str) -> Option<Vec<u8>> {
    let bytes = encoded.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }

    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    for (chunk_index, chunk) in bytes.chunks_exact(4).enumerate() {
        let is_last = chunk_index == bytes.len() / 4 - 1;
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            if !is_last || chunk[3] != b'=' || b & 0x0f != 0 {
                return None;
            }
            None
        } else {
            Some(base64_value(chunk[2])?)
        };
        let d = if chunk[3] == b'=' {
            if !is_last || c.is_some_and(|value| value & 0x03 != 0) {
                return None;
            }
            None
        } else {
            Some(base64_value(chunk[3])?)
        };

        decoded.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            decoded.push((b << 4) | (c >> 2));
            if let Some(d) = d {
                decoded.push((c << 6) | d);
            }
        } else if d.is_some() {
            return None;
        }
    }
    Some(decoded)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_base64, handle_connection};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn decodes_basic_auth_payload() {
        assert_eq!(
            decode_base64("YWxpY2U6c2VjcmV0"),
            Some(b"alice:secret".to_vec())
        );
        assert_eq!(decode_base64("YQ=="), Some(b"a".to_vec()));
        assert_eq!(decode_base64("YWI="), Some(b"ab".to_vec()));
    }

    #[test]
    fn rejects_invalid_base64() {
        assert_eq!(decode_base64("not base64"), None);
        assert_eq!(decode_base64("YQ=A"), None);
    }

    #[test]
    fn accepts_nonempty_basic_auth_credentials() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream).unwrap();
        });

        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .write_all(
                b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWxpY2U6c2VjcmV0\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("You are signed in!"));
    }

    #[test]
    fn unauthenticated_root_returns_login_xml() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream).unwrap();
        });

        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("application/xml"));
        assert!(response.contains("id=\"username\""));
        assert!(response.contains("id=\"password\""));
    }
}
