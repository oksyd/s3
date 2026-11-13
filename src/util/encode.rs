use crate::Error;

fn is_unreserved(byte: u8) -> bool {
    matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

pub(crate) fn aws_percent_encode(input: &str) -> String {
    aws_percent_encode_impl(input, false)
}

pub(crate) fn aws_percent_encode_path(input: &str) -> String {
    aws_percent_encode_impl(input, true)
}

fn aws_percent_encode_impl(input: &str, preserve_slash: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if is_unreserved(b) || (preserve_slash && b == b'/') {
            out.push(b as char);
            continue;
        }

        out.push('%');
        out.push(hex_upper(b >> 4));
        out.push(hex_upper(b & 0x0F));
    }
    out
}

fn hex_upper(nibble: u8) -> char {
    b"0123456789ABCDEF"[usize::from(nibble)] as char
}

pub(crate) fn canonical_query_string(params: &[(String, String)]) -> String {
    let mut items = params
        .iter()
        .map(|(k, v)| (aws_percent_encode(k), aws_percent_encode(v)))
        .collect::<Vec<_>>();

    items.sort_unstable();

    let mut out = String::new();
    for (idx, (k, v)) in items.into_iter().enumerate() {
        if idx > 0 {
            out.push('&');
        }
        out.push_str(&k);
        out.push('=');
        out.push_str(&v);
    }
    out
}

pub(crate) fn percent_decode(encoded: &str) -> Result<String, Error> {
    let input = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            let pair = input.get(index + 1..index + 3).ok_or_else(|| {
                Error::decode("invalid percent encoding in S3 list response", None)
            })?;
            let mut byte = [0];
            hex::decode_to_slice(pair, &mut byte).map_err(|error| {
                Error::decode(
                    "invalid percent encoding in S3 list response",
                    Some(Box::new(error)),
                )
            })?;
            decoded.push(byte[0]);
            index += 3;
        } else {
            decoded.push(input[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|error| Error::decode("invalid UTF-8 in S3 list response", Some(Box::new(error))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding_for_query_uses_rfc3986() {
        assert_eq!(aws_percent_encode("a+b c"), "a%2Bb%20c");
        assert_eq!(aws_percent_encode("~"), "~");
        assert_eq!(aws_percent_encode("/"), "%2F");
    }

    #[test]
    fn percent_encoding_for_path_preserves_slash() {
        assert_eq!(aws_percent_encode_path("a/b+c"), "a/b%2Bc");
        assert_eq!(aws_percent_encode_path("/a b"), "/a%20b");
    }

    #[test]
    fn percent_decoding_preserves_literals_and_rejects_corruption() {
        assert_eq!(
            percent_decode("%252F+%2b%00%e4%b8%ad").unwrap(),
            "%2F++\0中"
        );
        for invalid in ["%", "%1", "%zz", "%FF", "%C0%AF", "%E4%B8"] {
            assert!(percent_decode(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn canonical_query_string_sorts_and_encodes() {
        let params = vec![
            ("b".to_string(), "2".to_string()),
            ("a".to_string(), "1".to_string()),
            ("a".to_string(), "0".to_string()),
            ("space".to_string(), "a b".to_string()),
        ];
        assert_eq!(canonical_query_string(&params), "a=0&a=1&b=2&space=a%20b");
    }
}
