use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const SPINNER_INTERVAL: Duration = Duration::from_millis(80);

pub fn stderr_is_tty() -> bool {
    std::io::stderr().is_terminal()
}

pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

pub fn log_http_request(method: &str, url: &str) {
    if !stderr_is_tty() {
        eprintln!("{:>10} ---> {}", method, url);
    }
}

pub fn log_http_response(status: u16, url: &str) {
    if !stderr_is_tty() {
        eprintln!("{:>10} <--- {}", status, url);
    }
}

pub fn log_store(path: &std::path::Path) {
    if !stderr_is_tty() {
        eprintln!("{:>10} {}", "store", path.display());
    }
}

pub fn log_fresh(remote_date: &str, local_str: &str) {
    if !stderr_is_tty() {
        eprintln!("{:>10} remote={} > local={}", "fresh", remote_date, local_str);
    }
}

pub struct Spinner {
    idx: usize,
    last_tick: Instant,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            idx: 0,
            last_tick: Instant::now(),
        }
    }

    pub fn tick(&mut self) -> char {
        let now = Instant::now();
        if now.duration_since(self.last_tick) >= SPINNER_INTERVAL {
            self.idx = (self.idx + 1) % SPINNER_CHARS.len();
            self.last_tick = now;
        }
        SPINNER_CHARS[self.idx]
    }
}

pub fn status(spinner: &mut Spinner, msg: &str) {
    eprint!("\r\x1b[2K{} {}", spinner.tick(), msg);
    let _ = std::io::stderr().flush();
}

pub fn finish(msg: &str) {
    eprintln!("\r\x1b[2K\x1b[32m✓\x1b[0m {}", msg);
}
