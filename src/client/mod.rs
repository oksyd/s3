#[cfg(feature = "_async")]
mod async_client;
#[cfg(feature = "_blocking")]
mod blocking_client;
mod common;

#[cfg(feature = "_async")]
pub use async_client::{Client, ClientBuilder};
#[cfg(feature = "_blocking")]
pub use blocking_client::{BlockingClient, BlockingClientBuilder};
