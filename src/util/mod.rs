#[cfg(any(
    test,
    feature = "_async",
    feature = "_blocking",
    feature = "credentials-imds",
    feature = "credentials-sts"
))]
pub(crate) mod encode;
pub(crate) mod env;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
pub(crate) mod headers;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
pub(crate) mod md5;
pub(crate) mod redact;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
pub(crate) mod signing;
#[cfg(any(
    test,
    feature = "_async",
    feature = "_blocking",
    feature = "credentials-imds",
    feature = "credentials-sts"
))]
pub(crate) mod text;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
pub(crate) mod url;
pub(crate) mod validation;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
pub(crate) mod xml;
