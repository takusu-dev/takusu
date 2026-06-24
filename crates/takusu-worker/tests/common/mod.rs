use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const PORT: u16 = 8789;

pub struct WranglerGuard(pub Option<Child>);

impl Drop for WranglerGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
        wait_for_port_free();
    }
}

pub fn start_wrangler() -> WranglerGuard {
    wait_for_port_free();

    let child = Command::new("wrangler")
        .args([
            "dev",
            "--local",
            "--port",
            &PORT.to_string(),
            "--log-level",
            "warn",
        ])
        .env("TAKUSU_ROOT_TOKEN", "tsk_test_root_dev")
        .spawn()
        .expect("failed to start wrangler dev");

    wait_for_ready();
    WranglerGuard(Some(child))
}

fn wait_for_port_free() {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let addr = ("127.0.0.1", PORT)
            .to_socket_addrs()
            .ok()
            .and_then(|mut a| a.next());
        if let Some(addr) = addr
            && TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_err()
        {
            return;
        }
        sleep(Duration::from_millis(200));
    }
}

fn wait_for_ready() {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if let Ok((200, _)) = http_get("/health", None) {
            return;
        }
        sleep(Duration::from_millis(500));
    }
    panic!("wrangler dev did not become ready within 120s");
}

pub fn http_get(path: &str, auth_token: Option<&str>) -> Result<(u16, String), String> {
    let host = "127.0.0.1";
    let mut stream = TcpStream::connect((host, PORT)).map_err(|e| format!("connect: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

    let auth_line = auth_token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{PORT}\r\n{auth_line}Connection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write: {e}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("read: {e}"))?;

    let response_str = String::from_utf8_lossy(&response);
    let status_line = response_str.lines().next().unwrap_or("");
    let parts: Vec<&str> = status_line.split(' ').collect();
    let status_code: u16 = parts.get(1).unwrap_or(&"500").parse().unwrap_or(500);

    let body = response_str
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .to_string();

    Ok((status_code, body))
}
