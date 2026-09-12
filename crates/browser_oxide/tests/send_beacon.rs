use browser_oxide::{stealth::presets::chrome_148_macos, Page};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
struct BeaconRequest {
    content_type: Option<String>,
    body: Vec<u8>,
}

type ParsedRequest = (String, String, BTreeMap<String, String>, Vec<u8>);

fn read_request(stream: &mut TcpStream) -> Option<ParsedRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut data = Vec::with_capacity(2048);
    let mut buf = [0u8; 2048];
    let header_end = loop {
        match stream.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                data.extend_from_slice(&buf[..n]);
                if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
                if data.len() > 64 * 1024 {
                    return None;
                }
            }
            Err(_) => return None,
        }
    };

    let head = String::from_utf8_lossy(&data[..header_end]);
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while data.len() < header_end + content_length {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let body_end = (header_end + content_length).min(data.len());
    Some((method, path, headers, data[header_end..body_end].to_vec()))
}

fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

#[tokio::test]
async fn send_beacon_body_mime_semantics_match_chrome() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fixture");
    listener
        .set_nonblocking(true)
        .expect("nonblocking local fixture");
    let address = listener.local_addr().expect("local fixture address");
    let captured = Arc::new(Mutex::new(BTreeMap::<String, BeaconRequest>::new()));
    let server_capture = Arc::clone(&captured);

    let html = r#"<!doctype html><meta charset="utf-8"><title>beacon</title><script>
        const cases = [
          ['string', 'hello'],
          ['urlsp', new URLSearchParams('a=b&c=d')],
          ['buffer', new Uint8Array([1,2,3]).buffer],
          ['typed', new Uint8Array([4,5,6])],
          ['blobtyped', new Blob(['xyz'], {type:'application/x-test'})],
          ['blobempty', new Blob(['raw'])],
        ];
        globalThis.__beaconQueued = cases.map(([name, data]) =>
          navigator.sendBeacon('/b/' + name, data)
        );
    </script>"#;

    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut workers = Vec::new();
        while Instant::now() < deadline {
            if server_capture.lock().unwrap().len() >= 6 {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let worker_capture = Arc::clone(&server_capture);
                    workers.push(std::thread::spawn(move || {
                        let Some((method, path, headers, body)) = read_request(&mut stream) else {
                            return;
                        };
                        if method == "POST" && path.starts_with("/b/") {
                            worker_capture.lock().unwrap().insert(
                                path.trim_start_matches("/b/").to_string(),
                                BeaconRequest {
                                    content_type: headers.get("content-type").cloned(),
                                    body,
                                },
                            );
                            write_response(&mut stream, "204 No Content", "text/plain", b"");
                        } else {
                            write_response(&mut stream, "404 Not Found", "text/plain", b"");
                        }
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("local fixture accept failed: {error}"),
            }
        }
        for worker in workers {
            worker.join().expect("join local fixture connection");
        }
    });

    let url = format!("http://{address}/");
    let mut page = Page::from_html_with_url(html, &url, Some(chrome_148_macos()))
        .await
        .expect("create local sendBeacon fixture page");
    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__beaconQueued)")
            .expect("read beacon return values"),
        "[true,true,true,true,true,true]"
    );

    let beacon_deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < beacon_deadline {
        if captured.lock().unwrap().len() >= 6 {
            break;
        }
        let _ = page
            .event_loop()
            .run_until_idle(Duration::from_millis(50))
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    server.join().expect("join local fixture");

    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 6, "captured requests: {requests:#?}");

    assert_eq!(
        requests.get("string"),
        Some(&BeaconRequest {
            content_type: Some("text/plain;charset=UTF-8".to_string()),
            body: b"hello".to_vec(),
        })
    );
    assert_eq!(
        requests.get("urlsp"),
        Some(&BeaconRequest {
            content_type: Some("application/x-www-form-urlencoded;charset=UTF-8".to_string()),
            body: b"a=b&c=d".to_vec(),
        })
    );
    assert_eq!(
        requests.get("buffer"),
        Some(&BeaconRequest {
            content_type: None,
            body: vec![1, 2, 3],
        })
    );
    assert_eq!(
        requests.get("typed"),
        Some(&BeaconRequest {
            content_type: None,
            body: vec![4, 5, 6],
        })
    );
    assert_eq!(
        requests.get("blobtyped"),
        Some(&BeaconRequest {
            content_type: Some("application/x-test".to_string()),
            body: b"xyz".to_vec(),
        })
    );
    assert_eq!(
        requests.get("blobempty"),
        Some(&BeaconRequest {
            content_type: None,
            body: b"raw".to_vec(),
        })
    );
}
