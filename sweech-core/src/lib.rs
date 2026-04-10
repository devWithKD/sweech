pub mod auth;
pub mod context;
pub mod error;
pub mod handler;
pub mod response;

pub use auth::{AuthRequirement, UserClaims};
pub use context::{
    AppletContext, CacheContext, DbContext, QueueContext, RequestInfo, StorageContext,
};
pub use error::AppletError;
pub use error::Guard;
pub use handler::{Handler, HttpMethod, SweechResult};
pub use response::AppletResponse;

pub mod prelude {
    pub use crate::{
        AppletContext, AppletError, AppletResponse, AuthRequirement, Handler, HttpMethod,
        SweechResult, UserClaims,
    };
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
}
