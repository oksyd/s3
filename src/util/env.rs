use crate::{Error, Result};

pub(crate) fn optional_var(name: &'static str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(Error::invalid_config(format!(
            "{name} must be valid Unicode"
        ))),
    }
}

#[cfg(feature = "credentials-profile")]
pub(crate) fn optional_first_var(names: &[&'static str]) -> Result<Option<(&'static str, String)>> {
    optional_first_var_with(names, optional_var)
}

#[cfg(feature = "credentials-sts")]
pub(crate) fn optional_first_non_empty_var(
    names: &[&'static str],
) -> Result<Option<(&'static str, String)>> {
    optional_first_var_with(names, |name| {
        Ok(optional_var(name)?.filter(|value| !value.is_empty()))
    })
}

#[cfg(any(test, feature = "credentials-profile", feature = "credentials-sts"))]
fn optional_first_var_with<F>(
    names: &[&'static str],
    mut get: F,
) -> Result<Option<(&'static str, String)>>
where
    F: FnMut(&'static str) -> Result<Option<String>>,
{
    for name in names {
        if let Some(value) = get(name)? {
            return Ok(Some((name, value)));
        }
    }
    Ok(None)
}

pub(crate) fn required_var(name: &'static str) -> Result<String> {
    optional_var(name)?.ok_or_else(|| Error::invalid_config(format!("missing {name}")))
}

#[cfg(feature = "credentials-sts")]
pub(crate) fn required_non_empty_var(name: &'static str) -> Result<String> {
    match optional_var(name)? {
        Some(value) if !value.is_empty() => Ok(value),
        Some(_) => Err(Error::invalid_config(format!("{name} must not be empty"))),
        None => Err(Error::invalid_config(format!("missing {name}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_first_var_is_lazy() {
        let mut visited = Vec::new();
        let value = optional_first_var_with(&["PRIMARY", "FALLBACK"], |name| {
            visited.push(name);
            Ok((name == "PRIMARY").then(|| "value".to_string()))
        })
        .unwrap();

        assert_eq!(value, Some(("PRIMARY", "value".to_string())));
        assert_eq!(visited, ["PRIMARY"]);
    }

    #[test]
    fn optional_first_non_empty_var_skips_empty_values() {
        let mut visited = Vec::new();
        let value = optional_first_var_with(&["PRIMARY", "FALLBACK"], |name| {
            visited.push(name);
            Ok(match name {
                "PRIMARY" => Some(String::new()),
                "FALLBACK" => Some("fallback".to_string()),
                _ => None,
            }
            .filter(|value| !value.is_empty()))
        })
        .unwrap();

        assert_eq!(value, Some(("FALLBACK", "fallback".to_string())));
        assert_eq!(visited, ["PRIMARY", "FALLBACK"]);
    }

    #[test]
    fn optional_first_var_reports_first_error_before_fallback() {
        let err = optional_first_var_with(&["PRIMARY", "FALLBACK"], |name| {
            if name == "PRIMARY" {
                Err(Error::invalid_config("bad primary"))
            } else {
                Ok(Some("fallback".to_string()))
            }
        })
        .expect_err("primary error must stop fallback lookup");

        match err {
            Error::InvalidConfig { message } => assert!(message.contains("bad primary")),
            other => panic!("expected invalid config, got {other:?}"),
        }
    }
}
