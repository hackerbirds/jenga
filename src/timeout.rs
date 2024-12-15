use std::{marker::PhantomData, time::Duration};

use tokio::time::timeout;

use crate::Service;

/// A service that returns an Error if the
/// time of the request exceeds the given timeout duration
pub struct Timeout<R, T: Service<R>> {
    inner: T,
    timeout_duration: Duration,
    phantom: PhantomData<R>,
}

#[derive(Debug, PartialEq)]
pub enum TimeoutError<R, T: Service<R>> {
    ServiceError(T::Error),
    TimeoutError,
}

impl<R, T: Service<R>> Timeout<R, T> {
    pub fn new(service: T, timeout_duration: Duration) -> Self {
        Timeout {
            inner: service,
            timeout_duration,
            phantom: PhantomData,
        }
    }
}

impl<R, T: Service<R>> Service<R> for Timeout<R, T> {
    type Response = T::Response;
    type Error = TimeoutError<R, T>;
    async fn request(&self, msg: R) -> Result<Self::Response, Self::Error> {
        match timeout(self.timeout_duration, self.inner.request(msg)).await {
            Ok(res) => res.map_err(|e| TimeoutError::ServiceError(e)),
            Err(_) => Err(TimeoutError::TimeoutError),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::ops::Mul;

    use tokio::time::sleep;

    use super::*;

    #[derive(Debug, PartialEq)]
    pub struct TestTimeoutService {}

    impl Service<u64> for TestTimeoutService {
        type Response = u64;
        type Error = ();

        async fn request(&self, msg: u64) -> Result<Self::Response, Self::Error> {
            sleep(Duration::from_millis(msg)).await;
            if msg == 14 || msg == 18 {
                Err(())
            } else {
                Ok(msg.mul(2))
            }
        }
    }

    impl Debug for Timeout<u64, TestTimeoutService> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.inner.fmt(f)
        }
    }

    #[tokio::test]
    async fn timeout_test() {
        let service = TestTimeoutService {};

        assert_eq!(service.request(10).await.unwrap(), 20);
        assert!(service.request(14).await.is_err());
        assert!(service.request(18).await.is_err());
        assert_eq!(service.request(20).await.unwrap(), 40);

        let service_timeout = Timeout::new(service, Duration::from_millis(15));

        assert_eq!(service_timeout.request(10).await.unwrap(), 20);
        assert_eq!(
            service_timeout.request(14).await,
            Err(TimeoutError::ServiceError(()))
        );
        assert_eq!(
            service_timeout.request(18).await.unwrap_err(),
            TimeoutError::TimeoutError
        );
        assert_eq!(
            service_timeout.request(20).await.unwrap_err(),
            TimeoutError::TimeoutError
        );
    }
}
