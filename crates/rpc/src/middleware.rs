use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use std::sync::Arc;
use std::time::Duration;

/// Rate limiter middleware
#[derive(Clone)]
pub struct RateLimitLayer {
    pub limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl RateLimitLayer {
    /// Create a new rate limiter with requests per second
    pub fn new(requests_per_second: u32) -> Self {
        let quota = Quota::per_second(std::num::NonZeroU32::new(requests_per_second).unwrap());
        let limiter = Arc::new(RateLimiter::direct(quota));
        Self { limiter }
    }

    /// Create a new rate limiter with custom duration
    pub fn new_with_duration(requests: u32, duration: Duration) -> Self {
        let quota = Quota::with_period(duration)
            .unwrap()
            .allow_burst(std::num::NonZeroU32::new(requests).unwrap());
        let limiter = Arc::new(RateLimiter::direct(quota));
        Self { limiter }
    }
}

/// Rate limiting middleware handler
pub async fn rate_limit_middleware(
    limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    request: Request,
    next: Next,
) -> Response {
    match limiter.check() {
        Ok(_) => next.run(request).await,
        Err(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded. Please try again later.",
        )
            .into_response(),
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker state
struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<std::time::Instant>,
}

/// Circuit breaker for handling cascading failures
#[derive(Clone)]
pub struct CircuitBreaker {
    state: Arc<tokio::sync::RwLock<CircuitBreakerState>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    ///
    /// # Arguments
    /// * `failure_threshold` - Number of failures before opening circuit
    /// * `success_threshold` - Number of successes to close circuit from half-open
    /// * `timeout` - Time to wait before trying half-open state
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: Arc::new(tokio::sync::RwLock::new(CircuitBreakerState {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
            })),
            failure_threshold,
            success_threshold,
            timeout,
        }
    }

    /// Check if request should be allowed
    pub async fn allow_request(&self) -> bool {
        let mut state = self.state.write().await;

        match state.state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => state
                .last_failure_time
                .filter(|&last| last.elapsed() >= self.timeout)
                .map(|_| {
                    state.state = CircuitState::HalfOpen;
                    state.success_count = 0;
                    true
                })
                .unwrap_or(false),
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match state.state {
            CircuitState::Closed => state.failure_count = 0,
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.success_threshold {
                    state.state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;

        match state.state {
            CircuitState::Closed => {
                state.failure_count += 1;
                if state.failure_count >= self.failure_threshold {
                    state.state = CircuitState::Open;
                    state.last_failure_time = Some(std::time::Instant::now());
                }
            }
            CircuitState::HalfOpen => {
                state.state = CircuitState::Open;
                state.failure_count = 0;
                state.success_count = 0;
                state.last_failure_time = Some(std::time::Instant::now());
            }
            CircuitState::Open => {}
        }
    }

    /// Get current circuit state
    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.state
    }
}

/// Circuit breaker middleware handler
pub async fn circuit_breaker_middleware(
    breaker: Arc<CircuitBreaker>,
    request: Request,
    next: Next,
) -> Response {
    if !breaker.allow_request().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Circuit breaker is open. Service temporarily unavailable.",
        )
            .into_response();
    }

    let response = next.run(request).await;

    if response.status().is_success() {
        breaker.record_success().await;
    } else if response.status().is_server_error() {
        breaker.record_failure().await;
    }

    response
}

/// Request timeout middleware
pub async fn timeout_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let timeout_duration = Duration::from_secs(30);

    match tokio::time::timeout(timeout_duration, next.run(request)).await {
        Ok(response) => Ok(response),
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let breaker = CircuitBreaker::new(3, 2, Duration::from_millis(100));

        // Initially closed
        assert_eq!(breaker.get_state().await, CircuitState::Closed);

        // Record failures
        for _ in 0..3 {
            breaker.record_failure().await;
        }

        // Should be open now
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // Should not allow requests
        assert!(!breaker.allow_request().await);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open
        assert!(breaker.allow_request().await);
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);

        // Record successes
        breaker.record_success().await;
        breaker.record_success().await;

        // Should be closed now
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimitLayer::new(2); // 2 requests per second

        // First two requests should succeed
        assert!(limiter.limiter.check().is_ok());
        assert!(limiter.limiter.check().is_ok());

        // Third should be rate limited
        assert!(limiter.limiter.check().is_err());

        // Wait and try again
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(limiter.limiter.check().is_ok());
    }
}
