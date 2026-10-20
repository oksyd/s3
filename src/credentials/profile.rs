use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{auth::Credentials, error::Error};

pub(crate) fn profile_from_env() -> Result<String, Error> {
    if let Some((name, profile)) =
        crate::util::env::optional_first_var(&["AWS_PROFILE", "AWS_DEFAULT_PROFILE"])?
    {
        validate_profile_name(name, &profile)?;
        return Ok(profile);
    }

    Ok("default".to_string())
}

pub(crate) fn load_profile_credentials(profile: &str) -> Result<Credentials, Error> {
    validate_profile_name("profile", profile)?;
    let creds_ini = read_ini_file(&credentials_path()?)?;
    let config_ini = read_ini_file(&config_path()?)?;

    credentials_from_profiles(profile, &creds_ini, &config_ini)
}

type Profiles = HashMap<String, HashMap<String, String>>;

fn credentials_from_profiles(
    profile: &str,
    credentials: &Profiles,
    config: &Profiles,
) -> Result<Credentials, Error> {
    let profile_section_config = if profile == "default" {
        "default".to_string()
    } else {
        format!("profile {profile}")
    };

    // A key pair and its optional session token must come from the same file.
    let fields = credentials
        .get(profile)
        .filter(|fields| {
            [
                "aws_access_key_id",
                "aws_secret_access_key",
                "aws_session_token",
            ]
            .iter()
            .any(|key| fields.contains_key(*key))
        })
        .or_else(|| config.get(&profile_section_config))
        .ok_or_else(|| Error::invalid_config("profile has no static credentials"))?;
    let access_key_id = fields
        .get("aws_access_key_id")
        .ok_or_else(|| Error::invalid_config("missing aws_access_key_id in profile"))?;
    let secret_access_key = fields
        .get("aws_secret_access_key")
        .ok_or_else(|| Error::invalid_config("missing aws_secret_access_key in profile"))?;

    let mut creds = Credentials::new(access_key_id.as_str(), secret_access_key.as_str())?;
    if let Some(token) = fields.get("aws_session_token") {
        creds = creds.with_session_token(token.as_str())?;
    }

    Ok(creds)
}

fn validate_profile_name(name: &'static str, value: &str) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::invalid_config(format!("{name} must not be empty")));
    }
    if value.trim() != value {
        return Err(Error::invalid_config(format!(
            "{name} must not include leading or trailing whitespace"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::invalid_config(format!(
            "{name} must not contain control characters"
        )));
    }
    Ok(())
}

fn env_path(name: &'static str) -> Result<Option<PathBuf>, Error> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Err(Error::invalid_config(format!("{name} must not be empty")));
    }
    Ok(Some(PathBuf::from(value)))
}

fn home_dir() -> Result<Option<PathBuf>, Error> {
    if let Some(path) = env_path("HOME")? {
        return Ok(Some(path));
    }
    env_path("USERPROFILE")
}

fn default_aws_dir() -> Result<PathBuf, Error> {
    let home = home_dir()?.ok_or_else(|| Error::invalid_config("cannot determine home dir"))?;
    Ok(home.join(".aws"))
}

fn credentials_path() -> Result<PathBuf, Error> {
    if let Some(path) = env_path("AWS_SHARED_CREDENTIALS_FILE")? {
        return Ok(path);
    }
    Ok(default_aws_dir()?.join("credentials"))
}

fn config_path() -> Result<PathBuf, Error> {
    if let Some(path) = env_path("AWS_CONFIG_FILE")? {
        return Ok(path);
    }
    Ok(default_aws_dir()?.join("config"))
}

fn read_ini_file(path: &Path) -> Result<Profiles, Error> {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse_ini(&contents),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Profiles::new()),
        Err(err) => Err(Error::invalid_config(format!(
            "failed to read AWS profile file {}: {err}",
            path.display()
        ))),
    }
}

fn parse_ini(contents: &str) -> Result<Profiles, Error> {
    let mut sections = Profiles::new();
    let mut current: Option<String> = None;

    for (index, raw) in contents.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        let invalid_line =
            || Error::invalid_config(format!("invalid AWS profile syntax at line {}", index + 1));
        if line.starts_with('[') {
            let name = line
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(invalid_line)?;
            current = Some(name.to_owned());
            continue;
        }

        let section = current.as_ref().ok_or_else(invalid_line)?;
        let (k, v) = line.split_once('=').ok_or_else(invalid_line)?;

        let key = k.trim().to_ascii_lowercase();
        let value = v.trim().to_string();
        if key.is_empty() {
            return Err(invalid_line());
        }

        let fields = sections.entry(section.clone()).or_default();
        if matches!(
            key.as_str(),
            "aws_access_key_id" | "aws_secret_access_key" | "aws_session_token"
        ) && fields.contains_key(&key)
        {
            return Err(Error::invalid_config(format!(
                "duplicate {key} at line {}",
                index + 1
            )));
        }
        fields.insert(key, value);
    }

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ini_sections_and_keys() {
        let ini = r#"
; comment
[default]
aws_access_key_id = AKID
aws_secret_access_key= SECRET
aws_session_token = TOKEN

[profile dev]
aws_access_key_id = AKID2
aws_secret_access_key = SECRET2

ignored = outside
"#;

        let parsed = parse_ini(ini).unwrap();
        let credentials = credentials_from_profiles("default", &parsed, &Profiles::new()).unwrap();
        assert_eq!(credentials.access_key_id(), "AKID");
        assert_eq!(credentials.secret_access_key(), "SECRET");
        assert_eq!(credentials.session_token(), Some("TOKEN"));
        let credentials = credentials_from_profiles("dev", &Profiles::new(), &parsed).unwrap();
        assert_eq!(credentials.access_key_id(), "AKID2");
        assert_eq!(credentials.secret_access_key(), "SECRET2");
    }

    #[test]
    fn credential_files_are_selected_as_a_complete_group() {
        let config = parse_ini("[default]\naws_access_key_id=CONFIG\naws_secret_access_key=SECRET\naws_session_token=TOKEN").unwrap();
        for partial in [
            "aws_access_key_id=PARTIAL",
            "aws_secret_access_key=PARTIAL",
            "aws_session_token=PARTIAL",
            "aws_access_key_id=",
        ] {
            let credentials = parse_ini(&format!("[default]\n{partial}")).unwrap();
            assert!(credentials_from_profiles("default", &credentials, &config).is_err());
        }
        let credentials =
            parse_ini("[default]\naws_access_key_id=FILE\naws_secret_access_key=SECRET").unwrap();
        let chosen = credentials_from_profiles("default", &credentials, &config).unwrap();
        assert_eq!(chosen.access_key_id(), "FILE");
        assert_eq!(chosen.session_token(), None);
    }

    #[test]
    fn profile_file_errors_are_not_ignored() {
        const MARKER: &str = "S3_TEST_PROFILE_FILES";
        if let Ok(mode) = std::env::var(MARKER) {
            let result = load_profile_credentials("default");
            if mode == "config-only" {
                assert_eq!(result.unwrap().access_key_id(), "CONFIG");
            } else {
                assert!(result.is_err());
            }
            return;
        }

        let directory = std::env::temp_dir().join(format!("s3-profile-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let config = directory.join("config");
        std::fs::write(
            &config,
            "[default]\naws_access_key_id=CONFIG\naws_secret_access_key=SECRET",
        )
        .unwrap();
        for (mode, path) in [
            ("config-only", config.as_os_str()),
            ("unreadable", directory.as_os_str()),
            ("invalid-path", std::ffi::OsStr::new("")),
        ] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "credentials::profile::tests::profile_file_errors_are_not_ignored",
                ])
                .env(MARKER, mode)
                .env("AWS_SHARED_CREDENTIALS_FILE", directory.join("missing"))
                .env("AWS_CONFIG_FILE", path)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_profile_entries_are_not_silently_ignored() {
        for input in [
            "key=value",
            "[]",
            "[default",
            "[default]\nkey:value",
            "[default]\n=value",
            "[default]\naws_access_key_id=A\naws_access_key_id=B",
        ] {
            assert!(parse_ini(input).is_err(), "{input}");
        }
        let legacy = parse_ini("[default]\naws_access_key=A\naws_secret_key=B").unwrap();
        assert!(credentials_from_profiles("default", &legacy, &Profiles::new()).is_err());
    }

    #[test]
    fn profile_name_rejects_ambiguous_values() {
        validate_profile_name("AWS_PROFILE", "dev profile").unwrap();
        assert!(validate_profile_name("AWS_PROFILE", "").is_err());
        assert!(validate_profile_name("AWS_PROFILE", " dev").is_err());
        assert!(validate_profile_name("AWS_PROFILE", "dev ").is_err());
        assert!(validate_profile_name("AWS_PROFILE", "dev\nprofile").is_err());
    }

    #[test]
    fn load_profile_credentials_rejects_invalid_profile_before_io() {
        match load_profile_credentials(" dev") {
            Err(Error::InvalidConfig { message }) => assert!(message.contains("profile")),
            Err(other) => panic!("expected invalid config, got {other:?}"),
            Ok(_) => panic!("expected invalid profile to fail"),
        }
    }
}
