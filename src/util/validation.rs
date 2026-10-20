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
    if key.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::invalid_config(
            "object key must not contain ASCII control characters",
        ));
    }
    if key.split('/').any(|segment| matches!(segment, "." | "..")) {
        return Err(Error::invalid_config(
            "object key must not contain '.' or '..' path segments",
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
        assert!(validate_object_key("dir/../file.txt").is_err());
        assert!(validate_object_key("dir/./file.txt").is_err());
        assert!(validate_object_key("line\nbreak").is_err());
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
