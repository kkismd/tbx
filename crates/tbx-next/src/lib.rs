// Phase 1 defines the internal value contract before the stack and VM consume it.
#[allow(dead_code)]
mod value;

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
