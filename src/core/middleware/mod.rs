use std::future::Future;
use std::pin::Pin;

use crate::core::path::View;
use crate::core::request::Request;
use crate::core::response::AbstractResponse;

pub type Middleware = fn(Request, Option<View>) -> Pin<Box<dyn Future<Output=Box<dyn AbstractResponse>> + Send>>;

#[macro_export]
macro_rules! wrap_view {
    ($middleware_fn: ident) => {
            |request: Request, view: Option<View>| Box::pin($middleware_fn(request, view))
    }
}