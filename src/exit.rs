//! Exit codes, matching `gh`: 1 = error, 4 = authentication required, 8 = checks pending.
use std::fmt;

pub const AUTH_REQUIRED: i32 = 4;
pub const PENDING: i32 = 8;

/// An error that makes `bb` exit with `code` instead of 1. Display is the message.
#[derive(Debug)]
pub struct Exit {
    pub code: i32,
    pub msg: String,
}

impl Exit {
    pub fn new(code: i32, msg: impl Into<String>) -> anyhow::Error {
        anyhow::Error::new(Exit { code, msg: msg.into() })
    }
}

impl fmt::Display for Exit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for Exit {}

/// The exit code for an error: the first `Exit` in its chain, else 1.
pub fn code(e: &anyhow::Error) -> i32 {
    e.chain().find_map(|c| c.downcast_ref::<Exit>()).map_or(1, |x| x.code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert_eq!(code(&anyhow::anyhow!("plain")), 1);
        assert_eq!(code(&Exit::new(PENDING, "2 pending")), 8);
        assert_eq!(code(&Exit::new(AUTH_REQUIRED, "login").context("while listing")), 4);
        assert_eq!(Exit::new(4, "not logged in").to_string(), "not logged in");
    }
}
