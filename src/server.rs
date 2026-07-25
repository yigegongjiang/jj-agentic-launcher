use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::handoff::{snapshot, LoopState};

/// Bind a loopback observation server on an OS-assigned port and serve the
/// current loop state as JSON at `/handoff` (or `/`). The server thread is
/// detached — it blocks on `accept()` and is reclaimed on process exit.
pub fn start(state: Arc<Mutex<LoopState>>, mode: &'static str) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = handle(stream, &state, mode);
        }
    });

    Ok(port)
}

fn handle(
    mut stream: TcpStream,
    state: &Arc<Mutex<LoopState>>,
    mode: &'static str,
) -> std::io::Result<()> {
    // Read just enough to see the request line; headers/body are irrelevant.
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    let head = String::from_utf8_lossy(&buf[..n]);
    let path = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("");

    if path == "/handoff" || path == "/" {
        let body = {
            let guard = state.lock().unwrap_or_else(|e| e.into_inner());
            serde_json::to_string_pretty(&snapshot(&guard, mode))
                .unwrap_or_else(|_| "{}".to_string())
        };
        write_response(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            &body,
        )
    } else {
        write_response(&mut stream, "404 Not Found", "text/plain; charset=utf-8", "Not Found")
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let bytes = body.as_bytes();
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        bytes.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}
