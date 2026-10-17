//! The game's HTTP request objects (0xb8 bytes, created by `FUN_14020ced0`,
//! run by libcurl on a worker thread).
//!
//! [`crate::Mod::http_request`] sees each request just before it is handed to
//! the worker, which is the only safe moment to change its URL. libcurl is
//! built with `file://`, so redirecting to a local file works for any download.
//! Leaderboard writes never reach this point while mods are loaded (the
//! loader blocks them earlier).

use crate::msvc;

/// Offset of the URL `std::string` inside a request.
const URL: usize = 0x40;

pub struct HttpRequest(usize);

impl HttpRequest {
    pub fn from_ptr(p: usize) -> Option<Self> {
        (p != 0).then_some(Self(p))
    }

    pub fn as_ptr(&self) -> usize {
        self.0
    }

    pub fn url(&self) -> String {
        unsafe { msvc::read_string(self.0 + URL) }
    }

    pub fn set_url(&self, url: &str) -> bool {
        unsafe { msvc::assign_string(self.0 + URL, url) }
    }
}

/// `file:///` URL for a local path, percent-encoding everything libcurl's URL
/// parser could choke on (spaces in `Program Files (x86)`, for one).
pub fn file_url(path: &std::path::Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:///");
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
