// Phase 1 defines the internal value contract before the stack and VM consume it.
#[allow(dead_code)]
mod value;

// Phase 2 exposes the internal stack contract before the VM consumes it.
#[allow(dead_code)]
mod stack;

// Phase A for ADR #1368 centralizes validated word-name identity before the
// registry, primitive bootstrap, and compiler consume it.
#[allow(dead_code)]
mod name;

// Phase B for ADR #1368/#1369 keeps published executable word definitions
// separate from name binding, primitive bootstrap, builders, and mutable VM
// execution state.
#[allow(dead_code)]
mod word;

// Phase C for ADR #1368 keeps the current name binding table separate from word
// definitions, VM state, compiler surfaces, and future scalar/array storage.
#[allow(dead_code)]
mod binding;

// Phase D for ADR #1368 coordinates primitive bootstrap publication without
// merging primitive identity, word definitions, name binding, or VM state.
#[allow(dead_code)]
mod bootstrap;

pub const STATUS_MESSAGE: &str =
    "TBX Next is under development; language features are not implemented yet.";

pub fn status_message() -> &'static str {
    STATUS_MESSAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_message_reports_development_state() {
        assert!(status_message().contains("under development"));
    }
}
