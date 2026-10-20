//! Endpoint presets for common S3-compatible services.
//!
//! Presets only provide endpoint, region, and addressing defaults. Supply [`crate::Auth`]
//! separately when building a client.
//!
//! Start with:
//!
//! - [`aws_s3`](crate::providers::aws_s3) for AWS S3
//! - [`cloudflare_r2`](crate::providers::cloudflare_r2) for Cloudflare R2
//! - [`minio_local`](crate::providers::minio_local) for local MinIO development

use crate::{AddressingStyle, Error, Result};

/// Cloudflare R2 endpoint selection.
///
/// Jurisdiction-restricted buckets are only accessible through their jurisdiction's endpoint.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum R2Endpoint {
    /// Global endpoint: `https://<account_id>.r2.cloudflarestorage.com`
    Global,
    /// European Union endpoint: `https://<account_id>.eu.r2.cloudflarestorage.com`
    Eu,
    /// FedRAMP endpoint: `https://<account_id>.fedramp.r2.cloudflarestorage.com`
    Fedramp,
}

impl std::str::FromStr for R2Endpoint {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.trim() != value {
            return Err(Error::invalid_config(
                "R2 endpoint must not include leading or trailing whitespace",
            ));
        }
        match value.to_ascii_lowercase().as_str() {
            "global" => Ok(Self::Global),
            "eu" => Ok(Self::Eu),
            "fedramp" => Ok(Self::Fedramp),
            _ => Err(Error::invalid_config(
                "unknown R2 endpoint (expected: global, eu, fedramp)",
            )),
        }
    }
}

/// A preconfigured endpoint + region + addressing style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    endpoint: String,
    region: String,
    addressing_style: AddressingStyle,
}

impl Preset {
    /// Returns the service endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the signing region.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Returns the addressing style to use.
    pub fn addressing_style(&self) -> AddressingStyle {
        self.addressing_style
    }

    /// Builds an async client builder from the preset.
    #[cfg(feature = "async")]
    pub fn async_client_builder(&self) -> Result<crate::ClientBuilder> {
        crate::Client::builder(&self.endpoint).map(|b| {
            b.region(self.region.clone())
                .addressing_style(self.addressing_style)
        })
    }

    /// Builds a blocking client builder from the preset.
    #[cfg(feature = "blocking")]
    pub fn blocking_client_builder(&self) -> Result<crate::BlockingClientBuilder> {
        crate::BlockingClient::builder(&self.endpoint).map(|b| {
            b.region(self.region.clone())
                .addressing_style(self.addressing_style)
        })
    }
}

/// Builds a preset for AWS S3 using the region's endpoint.
pub fn aws_s3(region: impl AsRef<str>) -> Result<Preset> {
    let region = region.as_ref();
    crate::util::validation::validate_region("AWS region", region)?;

    let suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };

    Ok(Preset {
        endpoint: format!("https://s3.{region}.{suffix}"),
        region: region.to_string(),
        addressing_style: AddressingStyle::Auto,
    })
}

/// Builds a Cloudflare R2 preset.
///
/// Use [`R2Endpoint::Global`] for normal buckets, and [`R2Endpoint::Eu`] or
/// [`R2Endpoint::Fedramp`] for buckets created with Jurisdictional Restrictions.
///
/// This preset uses region `auto` and path-style addressing.
pub fn cloudflare_r2(account_id: impl AsRef<str>, endpoint: R2Endpoint) -> Result<Preset> {
    let account_id = account_id.as_ref();
    if account_id.is_empty() {
        return Err(Error::invalid_config("account_id must not be empty"));
    }
    if account_id.trim() != account_id {
        return Err(Error::invalid_config(
            "account_id must not include leading or trailing whitespace",
        ));
    }

    let invalid = account_id.len() > 63
        || account_id.starts_with('-')
        || account_id.ends_with('-')
        || !account_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-');
    if invalid {
        return Err(Error::invalid_config("account_id must be a DNS label"));
    }

    let account_id = account_id.to_ascii_lowercase();
    let endpoint = match endpoint {
        R2Endpoint::Global => format!("https://{account_id}.r2.cloudflarestorage.com"),
        R2Endpoint::Eu => format!("https://{account_id}.eu.r2.cloudflarestorage.com"),
        R2Endpoint::Fedramp => format!("https://{account_id}.fedramp.r2.cloudflarestorage.com"),
    };

    Ok(Preset {
        endpoint,
        region: "auto".to_string(),
        addressing_style: AddressingStyle::Path,
    })
}

/// Local MinIO preset for development.
pub fn minio_local() -> Preset {
    Preset {
        endpoint: "http://127.0.0.1:9000".to_string(),
        region: "us-east-1".to_string(),
        addressing_style: AddressingStyle::Path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_r2_uses_path_and_auto_region() {
        let preset = cloudflare_r2("123", R2Endpoint::Global).unwrap();
        assert_eq!(preset.endpoint(), "https://123.r2.cloudflarestorage.com");
        assert_eq!(preset.region(), "auto");
        assert_eq!(preset.addressing_style(), AddressingStyle::Path);
    }

    #[test]
    fn cloudflare_r2_endpoint_eu_uses_jurisdiction_host() {
        let preset = cloudflare_r2("123", R2Endpoint::Eu).unwrap();
        assert_eq!(preset.endpoint(), "https://123.eu.r2.cloudflarestorage.com");
        assert_eq!(preset.region(), "auto");
        assert_eq!(preset.addressing_style(), AddressingStyle::Path);
    }

    #[test]
    fn cloudflare_r2_endpoint_fedramp_uses_jurisdiction_host() {
        let preset = cloudflare_r2("123", R2Endpoint::Fedramp).unwrap();
        assert_eq!(
            preset.endpoint(),
            "https://123.fedramp.r2.cloudflarestorage.com"
        );
        assert_eq!(preset.region(), "auto");
        assert_eq!(preset.addressing_style(), AddressingStyle::Path);
    }

    #[test]
    fn aws_s3_uses_us_east_1_regional_endpoint() {
        let preset = aws_s3("us-east-1").unwrap();
        assert_eq!(preset.endpoint(), "https://s3.us-east-1.amazonaws.com");
        assert_eq!(preset.region(), "us-east-1");
        assert_eq!(preset.addressing_style(), AddressingStyle::Auto);
    }

    #[test]
    fn aws_s3_cn_uses_cn_suffix() {
        let preset = aws_s3("cn-north-1").unwrap();
        assert_eq!(preset.endpoint(), "https://s3.cn-north-1.amazonaws.com.cn");
    }

    #[test]
    fn presets_reject_outer_whitespace() {
        assert!(aws_s3(" us-east-1").is_err());
        assert!(aws_s3("us east 1").is_err());
        assert!(aws_s3("us/east/1").is_err());
        assert!(cloudflare_r2(" 123", R2Endpoint::Global).is_err());
        assert!(cloudflare_r2("a".repeat(64), R2Endpoint::Global).is_err());
        assert!(" eu".parse::<R2Endpoint>().is_err());
    }
}
