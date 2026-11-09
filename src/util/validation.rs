use crate::{Error, Result};

pub(crate) fn validate_region(name: &'static str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::invalid_config(format!("{name} must not be empty")));
    }
    if value.trim() != value {
        return Err(Error::invalid_config(format!(
            "{name} must not include leading or trailing whitespace"
        )));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(Error::invalid_config(format!(
            "{name} must contain only lowercase ASCII letters, digits, or '-'"
        )));
    }
    Ok(())
}

pub(crate) fn validate_object_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(Error::invalid_config("object key must not be empty"));
    }
    Ok(())
}

pub(crate) fn validate_xml_object_key(key: &str) -> Result<()> {
    validate_object_key(key)?;
    if key.chars().any(|c| {
        matches!(c, '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(Error::invalid_config(
            "object key contains characters that XML 1.0 cannot represent",
        ));
    }
    Ok(())
}

pub(crate) fn validate_version_id(version_id: &str) -> Result<()> {
    if version_id.is_empty() {
        return Err(Error::invalid_config("version_id must not be empty"));
    }
    if version_id.trim() != version_id {
        return Err(Error::invalid_config(
            "version_id must not include leading or trailing whitespace",
        ));
    }
    if version_id.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::invalid_config(
            "version_id must not contain ASCII control characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_object_key() {
        validate_object_key("dir/file.txt").unwrap();
        assert!(validate_object_key("").is_err());
        validate_object_key("dir/../file.txt").unwrap();
        validate_object_key("dir/./file.txt").unwrap();
        validate_object_key("line\nbreak\0").unwrap();
    }

    #[test]
    fn validates_version_id() {
        validate_version_id("version-1").unwrap();
        assert!(validate_version_id("").is_err());
        assert!(validate_version_id(" version").is_err());
        assert!(validate_version_id("version ").is_err());
        assert!(validate_version_id("line\nbreak").is_err());
    }
}
