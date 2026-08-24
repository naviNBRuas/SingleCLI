//! Best-effort rate-limit detection for a finished task's output. This is
//! deliberately generic rather than a per-agent table of exact error
//! strings: this project only claims specifics it has directly verified
//! (see e.g. `registry.rs`'s bootstrap-install commands), and nobody has
//! gone through all 20+ supported agent CLIs' real rate-limit output to
//! confirm exact wording. A small set of case-insensitive substrings that
//! commonly show up across HTTP APIs and CLI tools when a quota/rate
//! limit is hit is a genuinely honest middle ground: it will miss some
//! real rate limits (an agent phrasing it unusually) and it's not
//! infallible, but it won't fabricate confidence it doesn't have.
//! Paired with `single_core::account::AccountStatus::RateLimited`, which
//! is always authoritative when a user (or a previous detection) has
//! already set it.

const SIGNALS: &[&str] = &["rate limit", "rate_limit", "ratelimit", "429", "quota exceeded", "quota_exceeded", "too many requests", "usage limit"];

/// Scans a failed/timed-out run's combined output for a rate-limit signal.
pub fn looks_like_rate_limit(text: &str) -> bool {
    let lower = text.to_lowercase();
    SIGNALS.iter().any(|signal| lower.contains(signal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_rate_limit_phrasing_case_insensitively() {
        assert!(looks_like_rate_limit("Error: Rate limit exceeded, please try again later"));
        assert!(looks_like_rate_limit("HTTP 429 Too Many Requests"));
        assert!(looks_like_rate_limit("QUOTA_EXCEEDED for this billing period"));
    }

    #[test]
    fn ordinary_failures_are_not_flagged() {
        assert!(!looks_like_rate_limit("error: file not found"));
        assert!(!looks_like_rate_limit("panic: index out of bounds"));
        assert!(!looks_like_rate_limit(""));
    }
}
