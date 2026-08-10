use crate::name::NormalizedName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicationNameError {
    ReservedName,
}

/// Validates a normal name before it is published into the shared namespace.
///
/// ADR #1370 reserves VAR/LET for source forms, so ordinary words, variables,
/// and future binding kinds must reject those spellings before committing any
/// word ID, storage slot, or binding entry.
pub(crate) fn validate_publication_name(name: &NormalizedName) -> Result<(), PublicationNameError> {
    match name.as_str() {
        "VAR" | "LET" => Err(PublicationNameError::ReservedName),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    #[test]
    fn rejects_reserved_case_variants() {
        for input in ["VAR", "var", "Var", "LET", "let", "Let"] {
            assert_eq!(
                validate_publication_name(&name(input)),
                Err(PublicationNameError::ReservedName),
                "{input:?} should be reserved"
            );
        }
    }

    #[test]
    fn accepts_ordinary_names_that_contain_reserved_spellings() {
        for input in ["VAR1", "LETTER", "_LET"] {
            assert_eq!(validate_publication_name(&name(input)), Ok(()));
        }
    }
}
