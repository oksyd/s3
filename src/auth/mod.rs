mod cache;
mod model;
mod provider;

#[cfg(test)]
mod tests;

#[cfg(any(feature = "_async", feature = "_blocking"))]
pub use cache::CachedProvider;
pub use model::{AddressingStyle, Auth, Credentials, CredentialsSnapshot, Region};
#[cfg(feature = "_async")]
pub use provider::CredentialsFuture;
pub use provider::{CredentialsProvider, DynCredentialsProvider};
