use crate::{Error, Result};

pub(crate) fn decode_utf8_response_body(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes)
        .map_err(|e| Error::decode("response body is not valid UTF-8", Some(Box::new(e))))
}

#[cfg(any(test, feature = "credentials-imds"))]
pub(crate) fn decode_single_line_response_body<'a>(
    name: &'static str,
    bytes: &'a [u8],
) -> Result<&'a str> {
    let text = decode_utf8_response_body(bytes)?;
    let text = strip_trailing_line_ending(text);
    if text.is_empty() {
        return Err(Error::decode(format!("{name} response is empty"), None));
    }
    if text.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(Error::decode(
            format!("{name} response must be a single line"),
            None,
        ));
    }
    Ok(text)
}

#[cfg(any(test, feature = "credentials-imds", feature = "credentials-sts"))]
pub(crate) fn strip_trailing_line_ending(value: &str) -> &str {
    if let Some(value) = value.strip_suffix("\r\n") {
        value
    } else if let Some(value) = value.strip_suffix('\n') {
        value
    } else if let Some(value) = value.strip_suffix('\r') {
        value
    } else {
        value
    }
}

pub(crate) fn truncate_snippet(body: &str, max_len: usize) -> String {
    if body.len() <= max_len {
        return body.to_string();
    }

    let mut out = body[..body.floor_char_boundary(max_len)].to_string();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_valid_utf8_response_body() {
        assert_eq!(
            decode_utf8_response_body("hello".as_bytes()).unwrap(),
            "hello"
        );
    }

    #[test]
    fn rejects_invalid_utf8_response_body() {
        let err = decode_utf8_response_body(&[0xff]).expect_err("invalid UTF-8 must fail");
        match err {
            Error::Decode { message, .. } => assert!(message.contains("UTF-8")),
            other => panic!("expected Decode error, got {other:?}"),
        }
    }

    #[test]
    fn decodes_single_line_response_body_with_one_optional_line_ending() {
        assert_eq!(
            decode_single_line_response_body("token", b"abc\r\n").unwrap(),
            "abc"
        );
        assert_eq!(
            decode_single_line_response_body("token", b"abc\n").unwrap(),
            "abc"
        );
        assert!(decode_single_line_response_body("token", b"abc\n\n").is_err());
        assert!(decode_single_line_response_body("token", b"abc\rdef").is_err());
        assert!(decode_single_line_response_body("token", b"").is_err());
    }

    #[test]
    fn truncates_ascii_without_panic() {
        let body = "a".repeat(10);
        assert_eq!(truncate_snippet(&body, 10), body);
        assert_eq!(truncate_snippet(&body, 5), "aaaaa...");
    }

    #[test]
    fn truncates_utf8_safely() {
        let body = "你好，世界".repeat(10);
        let out = truncate_snippet(&body, 5);
        assert!(out.ends_with("..."));
        assert!(out.len() > 3);
        assert!(out.len() <= 5 + 3);
    }
}
