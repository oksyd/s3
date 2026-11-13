#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod encode;
pub(crate) mod env;
#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod headers;
#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod md5;
pub(crate) mod redact;
#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod signing;
#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod text;
#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod url;
pub(crate) mod validation;
#[cfg(any(feature = "_async", feature = "_blocking"))]
pub(crate) mod xml;
