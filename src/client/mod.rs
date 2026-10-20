#[cfg(feature = "async")]
mod async_client;
#[cfg(feature = "blocking")]
mod blocking_client;
mod common;

#[cfg(feature = "async")]
pub use async_client::{Client, ClientBuilder};
#[cfg(feature = "blocking")]
pub use blocking_client::{BlockingClient, BlockingClientBuilder};
