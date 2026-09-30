//! Cooperative monotonic deadlines shared by verification and execution.
use anyhow::Result;
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
#[error("total_timeout")]
pub(crate) struct TotalTimeout;

pub(crate) fn from_budget(now: Instant, budget: Duration) -> Option<Instant> {
    now.checked_add(budget)
}

pub(crate) fn check_at(deadline: Option<Instant>, now: Instant) -> Result<()> {
    if deadline.is_some_and(|deadline| now >= deadline) {
        return Err(TotalTimeout.into());
    }
    Ok(())
}

pub(crate) fn check(deadline: Option<Instant>) -> Result<()> {
    check_at(deadline, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_time_covers_boundary_unbounded_and_overflow() {
        let start = Instant::now();
        let end = from_budget(start, Duration::from_secs(1)).unwrap();
        assert!(check_at(Some(end), start).is_ok());
        assert!(check_at(Some(end), end).unwrap_err().is::<TotalTimeout>());
        assert!(check_at(Some(end), end + Duration::from_secs(1)).is_err());
        assert!(check_at(None, end).is_ok());
        assert!(from_budget(start, Duration::MAX).is_none());
        assert!(check(Some(start)).is_err());
    }
}
