pub(crate) fn redact_value(value: &str) -> String {
    let value = value.trim();
    if value.chars().nth(12).is_none() {
        return "<redacted>".to_string();
    }

    let head_end = value.char_indices().nth(2).unwrap().0;
    let tail_start = value.char_indices().rev().nth(1).unwrap().0;
    let head = &value[..head_end];
    let tail = &value[tail_start..];
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_values_consistently() {
        assert_eq!(redact_value(""), "<redacted>");
        assert_eq!(redact_value("   "), "<redacted>");
        assert_eq!(redact_value("12345678"), "<redacted>");
        assert_eq!(redact_value("123456789"), "<redacted>");
        assert_eq!(redact_value("123456789012"), "<redacted>");
        assert_eq!(redact_value("1234567890123"), "12...23");
        assert_eq!(redact_value("一二三四五六七八九十甲乙"), "<redacted>");
        assert_eq!(redact_value(" 一二三四五六七八九十甲乙丙 "), "一二...乙丙");
    }
}
