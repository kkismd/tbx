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
