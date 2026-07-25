/// Validated word name for TBX Next dictionary binding.
///
/// ADR #1368 fixes word names to `[A-Za-z_][A-Za-z0-9_]*\??` and uses
/// ASCII-uppercase spelling for case-insensitive binding identity. This module
/// is only a word-name construction boundary; it must not be reused as a
/// general string, source text, token, or diagnostic normalizer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NormalizedName {
    text: Box<str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameError {
    InvalidWordName,
}

impl NormalizedName {
    pub(crate) fn new(input: &str) -> Result<Self, NameError> {
        validate_word_name(input)?;

        Ok(Self {
            text: input.to_ascii_uppercase().into_boxed_str(),
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }
}

fn validate_word_name(input: &str) -> Result<(), NameError> {
    let bytes = input.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return Err(NameError::InvalidWordName);
    };

    if !is_word_initial(first) {
        return Err(NameError::InvalidWordName);
    }

    let last_index = bytes.len() - 1;

    for (offset, &byte) in rest.iter().enumerate() {
        let index = offset + 1;

        if byte == b'?' {
            if index != last_index {
                return Err(NameError::InvalidWordName);
            }

            continue;
        }

        if !is_word_body(byte) {
            return Err(NameError::InvalidWordName);
        }
    }

    Ok(())
}

fn is_word_initial(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_word_body(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    #[test]
    fn accepts_minimum_valid_names() {
        assert_eq!(name("A").as_str(), "A");
        assert_eq!(name("_").as_str(), "_");
    }

    #[test]
    fn normalizes_ascii_letters_to_uppercase() {
        assert_eq!(name("foo").as_str(), "FOO");
        assert_eq!(name("FOO").as_str(), "FOO");
        assert_eq!(name("Foo123").as_str(), "FOO123");
        assert_eq!(name("_temp_value").as_str(), "_TEMP_VALUE");
    }

    #[test]
    fn preserves_digits_underscore_and_trailing_question_mark() {
        assert_eq!(name("A1_B2").as_str(), "A1_B2");
        assert_eq!(name("ready?").as_str(), "READY?");
        assert_eq!(name("IS_EMPTY?").as_str(), "IS_EMPTY?");
    }

    #[test]
    fn rejects_invalid_word_names_without_returning_partial_names() {
        for invalid in [
            "",
            "1ABC",
            "?",
            "?READY",
            "IS?READY",
            "READY??",
            "READY?1",
            "A-B",
            "A B",
            "A\nB",
            "A\tB",
            "é",
            "Ａ",
            "NAMEé",
            "NAME😀",
            "VALID-BAD",
            "VALID BAD",
        ] {
            assert_eq!(
                NormalizedName::new(invalid),
                Err(NameError::InvalidWordName),
                "{invalid:?} should be rejected"
            );
        }
    }

    #[test]
    fn case_variants_are_the_same_binding_identity() {
        assert_eq!(name("foo"), name("Foo"));
        assert_eq!(name("Foo"), name("FOO"));
    }

    #[test]
    fn normalized_name_can_be_used_as_hash_map_key() {
        let mut bindings = HashMap::new();
        bindings.insert(name("foo"), 42);

        assert_eq!(bindings.get(&name("FOO")), Some(&42));
        assert_eq!(bindings.get(&name("Foo")), Some(&42));
    }

    #[test]
    fn unicode_case_conversion_is_not_applied() {
        assert_eq!(
            NormalizedName::new("straße"),
            Err(NameError::InvalidWordName)
        );
        assert_eq!(
            NormalizedName::new("istanbulİ"),
            Err(NameError::InvalidWordName)
        );
    }

    #[test]
    fn module_does_not_implicitly_normalize_general_strings() {
        let literal = "hello, ready?";

        assert_eq!(literal, "hello, ready?");
        assert_eq!(
            NormalizedName::new(literal),
            Err(NameError::InvalidWordName)
        );
    }
}
