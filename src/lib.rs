#[cfg(feature = "rate_limit")]
pub mod rate_limit;
#[cfg(feature = "retry")]
pub mod retry;
#[cfg(feature = "timeout")]
pub mod timeout;

#[allow(async_fn_in_trait)]
pub trait Service<Request> {
    type Response;
    type Error;
    async fn request(&self, msg: Request) -> Result<Self::Response, Self::Error>;
}

pub trait Middleware<R, S: Service<R>>: Service<R> {
    fn inner_service(&self) -> &S;
}
