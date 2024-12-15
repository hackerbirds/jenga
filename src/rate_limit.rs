use std::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::Service;

/// A basic rate limiter that limits how many concurrent
/// requests can happen on a given service.
pub struct RateLimit<const LIMIT: usize, R, T: Service<R>> {
    inner: T,
    current: AtomicUsize,
    phantom: PhantomData<R>,
}

#[derive(Debug, PartialEq)]
pub enum RateLimitError<R, T: Service<R>> {
    ServiceError(T::Error),
    RateLimited,
}

impl<const LIMIT: usize, R: Clone, T: Service<R>> RateLimit<LIMIT, R, T> {
    pub fn new(service: T) -> Self {
        Self {
            inner: service,
            current: AtomicUsize::new(0),
            phantom: PhantomData,
        }
    }
}

impl<const LIMIT: usize, R: Clone, T: Service<R>> Service<R> for RateLimit<LIMIT, R, T> {
    type Response = T::Response;
    type Error = RateLimitError<R, T>;
    async fn request(&self, msg: R) -> Result<Self::Response, Self::Error> {
        if let Err(_) = self
            .current
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                if v >= LIMIT {
                    None
                } else {
                    Some(v + 1)
                }
            })
        {
            return Err(RateLimitError::RateLimited);
        }

        let resp = self.inner.request(msg.clone()).await;

        self.current.fetch_sub(1, Ordering::Relaxed);

        resp.map_err(|e| RateLimitError::ServiceError(e))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{join, time::sleep};

    use super::*;

    #[derive(Debug)]
    pub struct TestRateLimitService {}

    impl Service<()> for TestRateLimitService {
        type Response = ();
        type Error = ();

        async fn request(&self, _msg: ()) -> Result<Self::Response, Self::Error> {
            sleep(Duration::from_millis(100)).await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn retry_rate_limit() {
        {
            let service = TestRateLimitService {};

            let rate_limit_service = RateLimit::<1, _, _>::new(service);

            let (a, b) = join!(
                rate_limit_service.request(()),
                rate_limit_service.request(())
            );

            assert!(a.is_ok() && b.is_err() || a.is_err() && b.is_ok());

            sleep(Duration::from_millis(200)).await;

            // Works again
            assert!(rate_limit_service.request(()).await.is_ok());
        }
    }
}
