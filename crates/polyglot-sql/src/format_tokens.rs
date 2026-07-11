//! Internal helpers for converting date/time format token strings.

/// Convert format tokens using longest-token matching.
///
/// Unknown text is copied through unchanged. Empty input returns `None`,
/// matching the public `time::format_time` helper.
pub(crate) fn convert_format_tokens(input: &str, mapping: &[(&str, &str)]) -> Option<String> {
    if input.is_empty() {
        return None;
    }

    let chars: Vec<char> = input.chars().collect();
    let mut result = String::with_capacity(input.len());
    let mut index = 0;

    while index < chars.len() {
        let mut matched: Option<(&str, usize)> = None;

        for (source, target) in mapping {
            let source_len = source.chars().count();
            if source_len == 0 || index + source_len > chars.len() {
                continue;
            }

            if chars[index..index + source_len]
                .iter()
                .copied()
                .eq(source.chars())
                && matched
                    .map(|(_, matched_len)| source_len > matched_len)
                    .unwrap_or(true)
            {
                matched = Some((*target, source_len));
            }
        }

        if let Some((target, source_len)) = matched {
            result.push_str(target);
            index += source_len;
        } else {
            result.push(chars[index]);
            index += 1;
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::convert_format_tokens;

    const TEST_MAPPING: &[(&str, &str)] = &[
        ("TMMonth", "%B"),
        ("TMMon", "%b"),
        ("FMHH24", "%-H"),
        ("HH24", "%H"),
        ("FMDD", "%-d"),
        ("DD", "%d"),
        ("D", "%u"),
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("MI", "%M"),
        ("SS", "%S"),
    ];

    #[test]
    fn longest_token_match_wins() {
        assert_eq!(
            convert_format_tokens("FMHH24 HH24 FMDD DD D TMMonth TMMon", TEST_MAPPING),
            Some("%-H %H %-d %d %u %B %b".to_string())
        );
    }

    #[test]
    fn unknown_text_passes_through() {
        assert_eq!(
            convert_format_tokens("YYYY-mm-DD literal", TEST_MAPPING),
            Some("%Y-mm-%d literal".to_string())
        );
    }

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(convert_format_tokens("", TEST_MAPPING), None);
    }
}
