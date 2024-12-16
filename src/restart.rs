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

use std::{marker::PhantomData, ops::DerefMut};

use thiserror::Error;
use tokio::sync::Mutex;

use crate::Service;

#[derive(Debug, Error)]
pub enum RestartError<SE: core::error::Error, GE: core::error::Error> {
    #[error("{0}")]
    ServiceError(SE),
    #[error("Could not restart failed service: {0}. Original error: {1}")]
    RestartingFailed(GE, SE),
}

pub struct Restart<
    SR: Clone,
    SResp,
    SE: core::error::Error,
    S: Service<SR, Response = SResp, Error = SE>,
    GR: Clone,
    GE: core::error::Error,
    G: Service<GR, Response = S, Error = GE>,
> {
    service: Mutex<S>,
    generator: G,
    r: PhantomData<SR>,
    g_r: GR,
    s_resp: PhantomData<SResp>,
    e: PhantomData<SE>,
    g_e: PhantomData<GE>,
}

impl<
        SR: Clone,
        SResp,
        SE: core::error::Error,
        S: Service<SR, Response = SResp, Error = SE>,
        GR: Clone,
        GE: core::error::Error,
        G: Service<GR, Response = S, Error = GE>,
    > Restart<SR, SResp, SE, S, GR, GE, G>
{
    pub async fn new(generator: G, generator_msg: GR) -> Result<Self, GE> {
        let service = Mutex::new(generator.request(generator_msg.clone()).await?);

        Ok(Self {
            service,
            generator,
            r: PhantomData,
            g_r: generator_msg,
            s_resp: PhantomData,
            e: PhantomData,
            g_e: PhantomData,
        })
    }
}

impl<
        SR: Clone,
        SResp,
        SE: core::error::Error,
        S: Service<SR, Response = SResp, Error = SE>,
        GR: Clone,
        GE: core::error::Error,
        G: Service<GR, Response = S, Error = GE>,
    > Service<SR> for Restart<SR, SResp, SE, S, GR, GE, G>
{
    type Response = SResp;
    type Error = RestartError<SE, GE>;

    async fn request(&self, msg: SR) -> Result<Self::Response, Self::Error> {
        let mut lock = self.service.lock().await;
        match lock.request(msg.clone()).await {
            Err(e1) => {
                let new_service = self
                    .generator
                    .request(self.g_r.clone())
                    .await
                    .map_err(|e2| RestartError::<SE, GE>::RestartingFailed(e2, e1))?;

                let _ = std::mem::replace(lock.deref_mut(), new_service);

                let resp = lock
                    .request(msg)
                    .await
                    .map_err(|e| RestartError::<SE, GE>::ServiceError(e))?;

                Ok(resp)
            }
            ok => ok.map_err(|e| RestartError::<SE, GE>::ServiceError(e)),
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
        assert_eq!(restart.service.lock().await.id, 1);
        assert!(restart.request(2).await.is_ok(), "Value is OK");
        assert_eq!(
            restart.service.lock().await.id,
            1,
            "OK value did not cause a restart"
        );

        match restart.request(3).await.unwrap_err() {
            RestartError::ServiceError(_) => {
                assert_eq!(restart.service.lock().await.id, 2, "At this point the service should have restarted and inner id should be 2 instead of 1");
            }
            RestartError::RestartingFailed(_, _) => {
                panic!("Restart service failed and did not restart")
            }
        };
    }
}
