//! P12-2: Retry + circuit breaker (稳定性)
//!
//! 给 LLM adapter 加 retry + backoff + circuit breaker.
//!
//! ## 设计
//!
//! - `RetryPolicy` — max attempts / initial backoff / max backoff / jitter
//! - `is_retryable(&AdapterError) -> bool` — 哪些错误重试 (网络 / 5xx / rate limit)
//! - `retry_with_backoff<F, T, E>(policy, op) -> Result<T, E>` — 跑 op, 失败按 policy 重试
//! - `CircuitBreaker` — 简单 closed/open/half-open 状态机, 连续 N 次失败 → open (短路)
//!
//! ## 用法
//!
//! ```rust,ignore
//! use ma_harness_model::retry::{RetryPolicy, retry_with_backoff, is_retryable};
//!
//! let policy = RetryPolicy::default();
//! let result = retry_with_backoff(&policy, || async {
//!     adapter.complete(&req).await
//! }, |e| is_retryable(&e)).await;
//! ```

use std::time::Duration;
use thiserror::Error;

/// Retry policy (P12-2)
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最多 retry 次数 (含第一次, 业务方 default 3)
    pub max_attempts: u32,
    /// 初始 backoff (default 100ms)
    pub initial_backoff: Duration,
    /// 最大 backoff (default 5s)
    pub max_backoff: Duration,
    /// jitter 比例 (0.0 - 1.0, default 0.1 = ±10% 抖动)
    pub jitter_ratio: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            jitter_ratio: 0.1,
        }
    }
}

impl RetryPolicy {
    /// 业务方 aggressive: 1 attempt, 0 backoff (失败不重试)
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            jitter_ratio: 0.0,
        }
    }

    /// 业务方 custom policy
    pub fn new(max_attempts: u32, initial_backoff: Duration, max_backoff: Duration) -> Self {
        Self {
            max_attempts,
            initial_backoff,
            max_backoff,
            jitter_ratio: 0.1,
        }
    }
}

/// 算 backoff: 指数 backoff + jitter
///
/// 业务方 attempts = 1, 2, 3, ... → backoff = initial * 2^(attempt-1), 上限 max, ±jitter
pub fn backoff_for(policy: &RetryPolicy, attempt: u32) -> Duration {
    // attempt 0-based (0 = 第 1 次失败, 应该 0 backoff)
    // attempt 1 = 第 2 次失败, 应该 initial
    if attempt == 0 {
        return Duration::ZERO;
    }
    let exp = (attempt - 1).min(20); // 防 2^20 overflow
    let multiplier = 1u64 << exp;
    let base_ms = policy.initial_backoff.as_millis() as u64;
    let raw_ms = base_ms.saturating_mul(multiplier);
    let capped_ms = raw_ms.min(policy.max_backoff.as_millis() as u64);
    // jitter: ±jitter_ratio
    if policy.jitter_ratio > 0.0 {
        // 简单 linear jitter (业务方用 std rand 后续可换)
        let max_jitter = (capped_ms as f64 * policy.jitter_ratio) as u64;
        // deterministic jitter from attempt
        let jitter_ms = (attempt as u64 * 17) % (max_jitter * 2 + 1);
        if jitter_ms <= max_jitter {
            Duration::from_millis(capped_ms.saturating_sub(max_jitter - jitter_ms))
        } else {
            Duration::from_millis(capped_ms.saturating_add(jitter_ms - max_jitter))
        }
    } else {
        Duration::from_millis(capped_ms)
    }
}

/// 判断错误是否可重试 (跟 AdapterError 配合)
pub fn is_retryable(err: &crate::AdapterError) -> bool {
    use crate::AdapterError::*;
    match err {
        Http(_) => true,           // 网络错误, 重试
        Api { status, .. } => {
            // 5xx 服务端错误 + 408 request timeout 重试, 4xx (除 408) 不重试
            *status >= 500 || *status == 408
        }
        Auth { .. } => false,      // 401/403 不重试 (业务方需修 API key)
        RateLimit { .. } => true,  // 429 重试
        Parse(_) => false,         // 解析错误, 重试也不会变
        MissingField(_) => false,  // 缺字段, 重试也不会变
    }
}

/// Retry 错误: 包最后一层 err + 总 attempt 数
#[derive(Debug, Error)]
pub enum RetryError<E: std::fmt::Display> {
    /// 重试用尽, 最后一次 err
    #[error("retry exhausted after {attempts} attempts: {source}")]
    Exhausted {
        /// 实际跑的次数
        attempts: u32,
        /// 最后一层 err
        #[source]
        source: E,
    },
    /// 第 1 次就 fail (不可重试, 不重试)
    #[error("non-retryable error: {0}")]
    NonRetryable(E),
}

/// Retry helper: 跑 op, 失败按 policy 重试
///
/// `op` 是 async closure, 返回 `Result<T, E>`.
/// `should_retry` 决定 err 是否可重试 (e.g. `|e| is_retryable(&e)`)
pub async fn retry_with_backoff<F, Fut, T, E, S>(
    policy: &RetryPolicy,
    mut op: F,
    should_retry: S,
) -> Result<T, RetryError<E>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
    S: Fn(&E) -> bool,
{
    let mut last_err: Option<E> = None;
    for attempt in 1..=policy.max_attempts {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                if !should_retry(&e) {
                    return Err(RetryError::NonRetryable(e));
                }
                last_err = Some(e);
                if attempt < policy.max_attempts {
                    let delay = backoff_for(policy, attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    Err(RetryError::Exhausted {
        attempts: policy.max_attempts,
        source: last_err.expect("at least one attempt"),
    })
}

// ============================================================================
// Circuit Breaker
// ============================================================================

/// Circuit breaker 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Closed: 正常走 op
    Closed,
    /// Open: 短路, 直接返 Err
    Open,
    /// Half-open: 允许 1 个 probe 试试
    HalfOpen,
}

/// Circuit breaker (P12-2 v1)
///
/// 业务方跑 LLM adapter, 连续 N 次失败 → open, 短期不再打.
/// 过了 cooldown → half-open, 允许 1 个 probe; 成功 → closed, 失败 → open
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// 当前状态
    state: std::sync::Arc<std::sync::Mutex<CircuitInner>>,
    /// 连续失败阈值
    failure_threshold: u32,
    /// open 状态持续时间
    cooldown: Duration,
}

#[derive(Debug)]
struct CircuitInner {
    state: CircuitState,
    consecutive_failures: u32,
    last_failure: Option<std::time::Instant>,
}

impl CircuitBreaker {
    /// 业务方: 5 次连续失败 → open, 冷却 30s
    pub fn new(failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(CircuitInner {
                state: CircuitState::Closed,
                consecutive_failures: 0,
                last_failure: None,
            })),
            failure_threshold,
            cooldown,
        }
    }

    /// 当前状态 (考虑 cooldown)
    pub fn state(&self) -> CircuitState {
        let mut inner = self.state.lock().expect("circuit lock poisoned");
        // 如果 open 状态过了 cooldown, 转 half-open
        if inner.state == CircuitState::Open {
            if let Some(last) = inner.last_failure {
                if last.elapsed() >= self.cooldown {
                    inner.state = CircuitState::HalfOpen;
                }
            }
        }
        inner.state
    }

    /// 记录成功
    pub fn record_success(&self) {
        let mut inner = self.state.lock().expect("circuit lock poisoned");
        inner.consecutive_failures = 0;
        inner.state = CircuitState::Closed;
    }

    /// 记录失败
    pub fn record_failure(&self) {
        let mut inner = self.state.lock().expect("circuit lock poisoned");
        inner.consecutive_failures += 1;
        inner.last_failure = Some(std::time::Instant::now());
        if inner.consecutive_failures >= self.failure_threshold {
            inner.state = CircuitState::Open;
        }
    }

    /// 业务方跑 op 前 check, open → 返 Err (短路, 不打 op)
    pub fn allow(&self) -> bool {
        !matches!(self.state(), CircuitState::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn retry_policy_default() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_attempts, 3);
        assert_eq!(p.initial_backoff, Duration::from_millis(100));
        assert_eq!(p.max_backoff, Duration::from_secs(5));
    }

    #[test]
    fn retry_policy_no_retry() {
        let p = RetryPolicy::no_retry();
        assert_eq!(p.max_attempts, 1);
    }

    #[test]
    fn backoff_exponential() {
        let mut p = RetryPolicy::new(5, Duration::from_millis(100), Duration::from_secs(10));
        p.jitter_ratio = 0.0; // 关闭 jitter 让测试稳定
        // attempt 0 → 0
        assert_eq!(backoff_for(&p, 0), Duration::ZERO);
        // attempt 1 → 100ms
        assert_eq!(backoff_for(&p, 1), Duration::from_millis(100));
        // attempt 2 → 200ms
        assert_eq!(backoff_for(&p, 2), Duration::from_millis(200));
        // attempt 3 → 400ms
        assert_eq!(backoff_for(&p, 3), Duration::from_millis(400));
    }

    #[test]
    fn backoff_capped() {
        let mut p = RetryPolicy::new(20, Duration::from_millis(100), Duration::from_secs(1));
        p.jitter_ratio = 0.0; // 关闭 jitter 让测试稳定
        // attempt 10 → 100 * 2^9 = 51200ms, capped at 1000ms
        let b = backoff_for(&p, 10);
        assert_eq!(b, Duration::from_secs(1));
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_try() {
        let policy = RetryPolicy::default();
        let result: Result<i32, RetryError<&str>> = retry_with_backoff(
            &policy,
            || async { Ok::<i32, &str>(42) },
            |_| true,
        )
        .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_succeeds_on_third_try() {
        let policy = RetryPolicy::new(3, Duration::from_millis(1), Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let c2 = counter.clone();
        let result: Result<i32, RetryError<&str>> = retry_with_backoff(
            &policy,
            || async {
                let n = c2.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err("transient")
                } else {
                    Ok(99)
                }
            },
            |_| true,
        )
        .await;
        assert_eq!(result.unwrap(), 99);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_exhausts() {
        let policy = RetryPolicy::new(2, Duration::from_millis(1), Duration::from_millis(5));
        let result: Result<i32, RetryError<&str>> = retry_with_backoff(
            &policy,
            || async { Err::<i32, &str>("always fail") },
            |_| true,
        )
        .await;
        assert!(matches!(result, Err(RetryError::Exhausted { attempts: 2, .. })));
    }

    #[tokio::test]
    async fn retry_non_retryable() {
        let policy = RetryPolicy::new(3, Duration::from_millis(1), Duration::from_millis(5));
        let counter = Arc::new(AtomicU32::new(0));
        let c2 = counter.clone();
        let result: Result<i32, RetryError<&str>> = retry_with_backoff(
            &policy,
            || async {
                c2.fetch_add(1, Ordering::SeqCst);
                Err::<i32, &str>("fatal")
            },
            |e| *e != "fatal",  // 业务方决定哪些不可重试
        )
        .await;
        assert!(matches!(result, Err(RetryError::NonRetryable(_))));
        assert_eq!(counter.load(Ordering::SeqCst), 1); // 1 attempt only
    }

    #[test]
    fn circuit_breaker_closed_by_default() {
        let cb = CircuitBreaker::new(5, Duration::from_secs(30));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow());
    }

    #[test]
    fn circuit_breaker_opens_on_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed); // 2 < 3
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open); // 3 == threshold
        assert!(!cb.allow());
    }

    #[test]
    fn circuit_breaker_success_resets_failures() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        // 重新累计
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn circuit_breaker_half_open_after_cooldown() {
        let cb = CircuitBreaker::new(2, Duration::from_millis(50));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        // 50ms 后 → half-open
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.allow()); // half-open 允许 1 个 probe
        // 成功 → closed
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn is_retryable_classification() {
        use crate::AdapterError;
        // Auth (401) → 不重试
        let auth = AdapterError::Auth { status: 401, body: "x".into() };
        assert!(!is_retryable(&auth));
        // 500 → 重试
        let api_500 = AdapterError::Api { status: 500, body: "x".into() };
        assert!(is_retryable(&api_500));
        // 400 → 不重试
        let api_400 = AdapterError::Api { status: 400, body: "x".into() };
        assert!(!is_retryable(&api_400));
        // 408 → 重试
        let api_408 = AdapterError::Api { status: 408, body: "x".into() };
        assert!(is_retryable(&api_408));
        // 429 → 重试
        let rl = AdapterError::RateLimit { body: "x".into() };
        assert!(is_retryable(&rl));
        // Parse → 不重试
        let parse = AdapterError::Parse(serde_json::from_str::<serde_json::Value>("invalid").unwrap_err());
        assert!(!is_retryable(&parse));
    }
}
