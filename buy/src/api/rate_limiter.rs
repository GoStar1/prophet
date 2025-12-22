use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovRateLimiter,
};
use std::num::NonZeroU32;

pub struct RateLimiter {
    limiter: GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        let quota = Quota::per_minute(NonZeroU32::new(requests_per_minute).unwrap());
        Self {
            limiter: GovRateLimiter::direct(quota),
        }
    }

    pub async fn acquire(&self) {
        self.limiter.until_ready().await;
    }
}
