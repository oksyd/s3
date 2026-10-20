#[cfg(feature = "credentials-imds")]
pub(crate) mod imds;
#[cfg(feature = "credentials-profile")]
pub(crate) mod profile;
#[cfg(feature = "credentials-sts")]
pub(crate) mod sts;

#[cfg(any(feature = "credentials-imds", feature = "credentials-sts"))]
pub(super) fn parse_expiration(value: &str) -> crate::Result<time::OffsetDateTime> {
    use crate::Error;
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    if value.is_empty() {
        return Err(Error::decode("missing credentials expiration", None));
    }
    if value.trim() != value {
        return Err(Error::decode(
            "credentials expiration timestamp must not include leading or trailing whitespace",
            None,
        ));
    }
    OffsetDateTime::parse(value, &Rfc3339).map_err(|e| {
        Error::decode(
            "failed to parse credentials expiration timestamp",
            Some(Box::new(e)),
        )
    })
}
