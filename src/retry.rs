use std::marker::PhantomData;

#[cfg(feature = "retry_wait")]
use std::time::Duration;

#[cfg(feature = "retry_wait")]
use tokio::time::sleep;

use crate::Service;

/// Service that retries the request a certain
/// amount of times before failing.
pub struct Retry<const RETRY_COUNT: usize, R: Clone, T: Service<R>> {
    inner: T,
    #[cfg(feature = "retry_wait")]
    duration: Duration,
    phantom: PhantomData<R>,
}

impl<const RETRY_COUNT: usize, R: Clone, T: Service<R>> Retry<RETRY_COUNT, R, T> {
    pub fn instant(service: T) -> Retry<RETRY_COUNT, R, T> {
        Retry {
            inner: service,
            #[cfg(feature = "retry_wait")]
            duration: Duration::ZERO,
            phantom: PhantomData,
        }
    }

    #[cfg(feature = "retry_wait")]
    pub fn with_wait(service: T, duration: Duration) -> Retry<RETRY_COUNT, R, T> {
        Retry {
            inner: service,
            duration,
            phantom: PhantomData,
        }
    }
}

impl<const RETRY_COUNT: usize, R: Clone, T: Service<R>> Service<R> for Retry<RETRY_COUNT, R, T> {
    type Response = T::Response;
    type Error = T::Error;
    async fn request(&self, msg: R) -> Result<Self::Response, Self::Error> {
        let mut retries_left = RETRY_COUNT;
        loop {
            match self.inner.request(msg.clone()).await {
                Ok(ok) => return Ok(ok),
                Err(err) => {
                    if retries_left == 0 {
                        return Err(err);
                    } else {
                        retries_left -= 1;

                        #[cfg(feature = "retry_wait")]
                        {
                            sleep(self.duration).await;
                        }

                        continue;
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Mutex;

    use super::*;

    #[derive(Debug)]
    pub struct TestRetryService {
        counter: Mutex<usize>,
        limit: usize,
    }

    impl Service<()> for TestRetryService {
        type Response = ();
        type Error = ();

        async fn request(&self, _msg: ()) -> Result<Self::Response, Self::Error> {
            let mut counter_lock = self.counter.lock().unwrap();
            if counter_lock.lt(&self.limit) {
                *counter_lock += 1;
                Err(())
            } else {
                *counter_lock = 0;
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn retry_test() {
        {
            let service = TestRetryService {
                counter: Mutex::new(0),
                limit: 3,
            };

            let retry_service = Retry::<3, _, _>::instant(service);

            assert!(retry_service.request(()).await.is_ok());
        }

        {
            let service = TestRetryService {
                counter: Mutex::new(0),
                limit: 4,
            };

            let retry_service = Retry::<3, _, _>::instant(service);

            assert!(retry_service.request(()).await.is_err());
        }
    }
}
