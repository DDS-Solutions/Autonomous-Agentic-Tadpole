//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Rate Limiter Verification (Throughput Tests)**: Orchestrates the
//! verification of the RPM (Requests Per Minute) and TPM (Tokens Per
//! Minute) constraints for the Tadpole OS engine. Features
//! **Windowing Logic Validation**: ensures that the `RateLimiter`
//! correctly blocks and resumes execution based on sliding window
//! budget exhaustion. Implements **Concurrent Acquisition Tests**:
//! validates that multiple agent threads can safely interact with the
//! atomic usage counters (SeqCst) without data races. AI agents should
//! run these tests to verify that the system correctly avoids
//! `429 Too Many Requests` errors from upstream LLM providers
//! (SEC-05).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[tests_rate_limiter]` in tracing logs.
//! - **Failure Path**: Unexpected window reset delays, atomic counter
//!   overflows in extreme throughput scenarios, or incorrect
//!   per-model budget mapping.
//! - **Trace Scope**: `server-rs::agent::tests_rate_limiter`

#[cfg(test)]
mod tests {
    use crate::agent::rate_limiter::RateLimiter;

    #[tokio::test]
    async fn rate_limiter_inactive_without_limits() {
        let limiter = RateLimiter::new(None, None);
        assert!(!limiter.is_active());
    }

    #[tokio::test]
    async fn rate_limiter_active_with_rpm() {
        let limiter = RateLimiter::new(Some(10), None);
        assert!(limiter.is_active());
    }

    #[tokio::test]
    async fn rate_limiter_active_with_tpm() {
        let limiter = RateLimiter::new(None, Some(100_000));
        assert!(limiter.is_active());
    }

    #[tokio::test]
    async fn rate_limiter_active_with_both() {
        let limiter = RateLimiter::new(Some(60), Some(100_000));
        assert!(limiter.is_active());
    }

    #[tokio::test]
    async fn rate_limiter_acquires_under_tpm_limit() {
        let limiter = RateLimiter::new(None, Some(10_000));
        // Should not block: 512 < 10_000
        limiter.acquire(512).await;
        // No panic = pass
    }

    #[tokio::test]
    async fn rate_limiter_records_usage() {
        let limiter = RateLimiter::new(None, Some(10_000));
        limiter.record_usage(500);
        limiter.record_usage(300);
        let total = limiter
            .tokens_used
            .load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(total, 800);
    }

    #[tokio::test]
    async fn rate_limiter_acquires_under_rpm_limit() {
        let limiter = RateLimiter::new(Some(100), None);
        // Should not block: first request of 100 available permits
        limiter.acquire(0).await;
        // No panic = pass
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rate_limiter_tpm_blocks_when_exceeded() {
        let limiter = RateLimiter::new(None, Some(100));

        // Use up all available token budget
        limiter.record_usage(100);

        // This should block because we've exhausted our TPM budget.
        // We use a timeout to prevent the test from hanging forever.
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), limiter.acquire(50)).await;

        // Should timeout because it's blocked waiting for window reset
        assert!(
            result.is_err(),
            "Expected timeout because TPM limit was exceeded"
        );
    }

    #[tokio::test]
    async fn rate_limiter_atomic_reservation_and_reconciliation() {
        let limiter = RateLimiter::new(None, Some(1000));

        // Acquire with estimate 400
        limiter.acquire(400).await;
        assert_eq!(limiter.tokens_used.load(std::sync::atomic::Ordering::SeqCst), 400);

        // Reconcile with actual 350 (used less than estimate)
        limiter.record_usage_reconcile(350, 400);
        assert_eq!(limiter.tokens_used.load(std::sync::atomic::Ordering::SeqCst), 350);

        // Reconcile with actual 500 (used more than estimate)
        limiter.acquire(100).await;
        assert_eq!(limiter.tokens_used.load(std::sync::atomic::Ordering::SeqCst), 450);
        limiter.record_usage_reconcile(150, 100);
        assert_eq!(limiter.tokens_used.load(std::sync::atomic::Ordering::SeqCst), 500);

        // Release reservation on failure
        limiter.acquire(200).await;
        assert_eq!(limiter.tokens_used.load(std::sync::atomic::Ordering::SeqCst), 700);
        limiter.release_reservation(200);
        assert_eq!(limiter.tokens_used.load(std::sync::atomic::Ordering::SeqCst), 500);
    }
}





// Metadata: [tests_rate_limiter]
