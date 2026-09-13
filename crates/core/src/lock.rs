#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoLockPolicy {
    timeout_seconds: u64,
    last_activity: u64,
}

impl AutoLockPolicy {
    pub fn new(timeout_seconds: u64, now: u64) -> Self {
        Self {
            timeout_seconds,
            last_activity: now,
        }
    }
    pub fn record_activity(&mut self, now: u64) {
        self.last_activity = now;
    }
    pub fn is_expired(&self, now: u64) -> bool {
        self.timeout_seconds > 0 && now.saturating_sub(self.last_activity) >= self.timeout_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::AutoLockPolicy;

    #[test]
    fn expires_after_configured_idle_window_and_never_when_disabled() {
        let mut policy = AutoLockPolicy::new(900, 100);
        assert!(!policy.is_expired(999));
        assert!(policy.is_expired(1000));
        policy.record_activity(1000);
        assert!(!policy.is_expired(1001));
        assert!(!AutoLockPolicy::new(0, 100).is_expired(u64::MAX));
    }
}
