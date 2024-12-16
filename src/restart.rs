//! Restart consists of two services:
//! - A service S
//! - A generator service G that generates services S
//!
//! Only the generator service G is needed. S is automatically
//! created when constructing [`Restart`].
//!
//! If S fails, then G will create a new service S, replacing the old one,
//! and call that. Only one restart attempt is made, after that it will
//! fail normally and return [`RestartError::ServiceError`]. If restarting
//! fails, then [`RestartError::RestartingFailed`] is returned instead.

use std::{cell::RefCell, marker::PhantomData};

use thiserror::Error;

use crate::Service;

#[derive(Debug, Error)]
pub enum RestartError<E: core::error::Error, E2: core::error::Error> {
    #[error("{0}")]
    ServiceError(E),
    #[error("Could not restart failed service: {0}. Original error: {1}")]
    RestartingFailed(E2, E),
}

pub struct Restart<
    R: Clone,
    R2: Clone,
    A,
    E: core::error::Error,
    E2: core::error::Error,
    S: Service<R, Response = A, Error = E>,
    G: Service<R2, Response = S, Error = E2>,
> {
    service: RefCell<S>,
    generator: G,
    r: PhantomData<R>,
    r2: R2,
    a: PhantomData<A>,
    e: PhantomData<E>,
    e2: PhantomData<E2>,
}

impl<
        R: Clone,
        R2: Clone,
        A,
        E: core::error::Error,
        E2: core::error::Error,
        S: Service<R, Response = A, Error = E>,
        G: Service<R2, Response = S, Error = E2>,
    > Restart<R, R2, A, E, E2, S, G>
{
    pub async fn new(generator: G, generator_msg: R2) -> Result<Self, E2> {
        let service = RefCell::new(generator.request(generator_msg.clone()).await?);

        Ok(Self {
            service,
            generator,
            r: PhantomData,
            r2: generator_msg,
            a: PhantomData,
            e: PhantomData,
            e2: PhantomData,
        })
    }
}

impl<
        R: Clone,
        R2: Clone,
        A,
        E: core::error::Error,
        E2: core::error::Error,
        S: Service<R, Response = A, Error = E>,
        G: Service<R2, Response = S, Error = E2>,
    > Service<R> for Restart<R, R2, A, E, E2, S, G>
{
    type Response = A;
    type Error = RestartError<E, E2>;

    async fn request(&self, msg: R) -> Result<Self::Response, Self::Error> {
        let borrow = self.service.borrow();
        match borrow.request(msg.clone()).await {
            Err(e1) => {
                drop(borrow);

                let new_service = self
                    .generator
                    .request(self.r2.clone())
                    .await
                    .map_err(|e2| RestartError::<E, E2>::RestartingFailed(e2, e1))?;

                self.service.replace(new_service);

                let resp = self
                    .service
                    .borrow()
                    .request(msg)
                    .await
                    .map_err(|e| RestartError::<E, E2>::ServiceError(e))?;

                Ok(resp)
            }
            ok => ok.map_err(|e| RestartError::<E, E2>::ServiceError(e)),
        }
    }
}
#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use super::*;

    #[derive(Debug)]
    pub struct TestRestartService {
        id: usize,
        value: usize,
    }

    #[derive(Debug, Error)]
    pub enum FakeError {
        #[error("")]
        Error,
    }

    impl Service<u64> for TestRestartService {
        type Response = ();
        type Error = FakeError;

        async fn request(&self, msg: u64) -> Result<Self::Response, Self::Error> {
            if msg as usize == self.value {
                Ok(())
            } else {
                Err(FakeError::Error)
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct TestGeneratorService {
        counter: Arc<AtomicUsize>,
    }

    impl Service<usize> for TestGeneratorService {
        type Response = TestRestartService;
        type Error = FakeError;

        async fn request(&self, msg: usize) -> Result<Self::Response, Self::Error> {
            if msg > 1 {
                self.counter.fetch_add(1, Ordering::SeqCst);
                Ok(TestRestartService {
                    id: self.counter.load(Ordering::SeqCst),
                    value: msg,
                })
            } else {
                Err(FakeError::Error)
            }
        }
    }

    #[tokio::test]
    async fn test_restart_service() {
        let generator = TestGeneratorService {
            counter: Arc::new(AtomicUsize::new(0)),
        };

        assert!(
            Restart::new(generator.clone(), 1).await.is_err(),
            "This generator will not start the service"
        );

        let restart = Restart::new(generator, 2).await.unwrap();
        assert_eq!(restart.service.borrow().id, 1);
        assert!(restart.request(2).await.is_ok(), "Value is OK");
        assert_eq!(
            restart.service.borrow().id,
            1,
            "OK value did not cause a restart"
        );

        match restart.request(3).await.unwrap_err() {
            RestartError::ServiceError(_) => {
                assert_eq!(restart.service.borrow().id, 2, "At this point the service should have restarted and inner id should be 2 instead of 1");
            }
            RestartError::RestartingFailed(_, _) => {
                panic!("Restart service failed and did not restart")
            }
        };
    }
}
