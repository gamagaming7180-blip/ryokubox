//! Authentication, loopback guards and one-time pairing for Ryoku Box.
//!
//! GPL-3.0-or-later. Brand-new code, no lineage from the old ryokubox tree.

use std::net::SocketAddr;
use std::time::Instant;

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use rand::RngCore;
use subtle::ConstantTimeEq;

/// Lifetime of a single-use pairing code, in seconds.
pub const PAIR_TTL_SECS: u64 = 120;

/// State behind the loopback-only pairing console.
#[derive(Debug)]
pub struct PairState {
    /// The currently issued, single-use code (hex).
    pub code: Option<String>,
    /// When the current code stops being accepted.
    pub expires_at: Option<Instant>,
}

impl PairState {
    pub fn new() -> Self {
        Self { code: None, expires_at: None }
    }

    /// Regenerate the one-time code. Every console load replaces the old one.
    pub fn issue(&mut self) -> String {
        let code = random_hex(16);
        self.code = Some(code.clone());
        self.expires_at = Some(Instant::now() + std::time::Duration::from_secs(PAIR_TTL_SECS));
        code
    }

    /// Constant-time validation. On success the code is consumed (single use).
    pub fn redeem(&mut self, candidate: &str) -> bool {
        let Some(code) = self.code.as_deref() else { return false };
        let Some(exp) = self.expires_at else { return false };
        if Instant::now() > exp {
            self.code = None;
            self.expires_at = None;
            return false;
        }
        let ok = ct_eq(code, candidate);
        self.code = None;
        self.expires_at = None;
        ok
    }
}

/// True when two secrets are equal, compared in constant time (length included).
pub fn ct_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    // Length is not secret here (tokens are fixed width), but fold it in anyway.
    let len_eq = a.len().ct_eq(&b.len());
    let body = if a.len() == b.len() {
        a.ct_eq(b)
    } else {
        // Compare a against itself so the work stays data-independent.
        a.ct_eq(a)
    };
    bool::from(len_eq & body)
}

/// Generate a random lowercase hex secret of `bytes` random bytes.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    let mut out = String::with_capacity(bytes * 2);
    for b in buf {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Extract a `Bearer` token from request headers and compare it in constant time.
pub fn bearer_ok(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok()) else { return false };
    let token = value.strip_prefix("Bearer ").unwrap_or("");
    ct_eq(expected, token.trim())
}

/// Loopback-only guard used by the console, QR and token-rotation endpoints.
pub fn is_loopback(addr: ConnectInfo<SocketAddr>) -> bool {
    addr_is_loopback(addr.0)
}

/// Same testable core as [`is_loopback`] for unit tests and other call sites.
pub fn addr_is_loopback(addr: SocketAddr) -> bool {
    addr.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn constant_time_check_accepts_exact_match() {
        let secret = "a3f5c1e09b2d4f6a";
        assert!(ct_eq(secret, secret));
    }

    #[test]
    fn constant_time_check_rejects_wrong_and_different_length() {
        let secret = "a3f5c1e09b2d4f6a";
        assert!(!ct_eq(secret, "a3f5c1e09b2d4f6b"));
        assert!(!ct_eq(secret, "short"));
        assert!(!ct_eq(secret, ""));
    }

    #[test]
    fn bearer_header_parsing() {
        let mut h = HeaderMap::new();
        h.insert(axum::http::header::AUTHORIZATION, "Bearer tok123".parse().unwrap());
        assert!(bearer_ok(&h, "tok123"));
        assert!(!bearer_ok(&h, "tok124"));
        h.insert(axum::http::header::AUTHORIZATION, "Basic tok123".parse().unwrap());
        assert!(!bearer_ok(&h, "tok123"));
    }

    #[test]
    fn loopback_guard_distinguishes_lan() {
        let lo = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8875);
        let lan = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 42)), 8875);
        assert!(addr_is_loopback(lo));
        assert!(!addr_is_loopback(lan));
    }

    #[test]
    fn pairing_code_is_single_use_and_expires() {
        let mut p = PairState::new();
        let code = p.issue();
        assert!(p.redeem(&code));
        assert!(!p.redeem(&code), "second redeem must fail");
        let code2 = p.issue();
        p.expires_at = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(!p.redeem(&code2), "expired code must fail");
    }
}
