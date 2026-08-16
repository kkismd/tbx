// Phase 1 defines the internal value contract before the stack and VM consume it.
#[allow(dead_code)]
mod value;

// Phase 4 for ADR #1367 keeps the typed shared instruction sequence outside
// mutable VM execution state and exposes only a read-only fetch boundary.
#[allow(dead_code)]
mod instruction;

// Phase 2 exposes the internal stack contract before the VM consumes it.
#[allow(dead_code)]
mod stack;

// Phase 5 for ADR #1367 adds mutable VM execution state and the one-instruction
// execution boundary over a read-only instruction view.
#[allow(dead_code)]
mod vm;

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

// Phase E for ADR #1368/#1369 coordinates explicit word redefinition as a
// separate publication boundary from ordinary registration and bootstrap.
#[allow(dead_code)]
mod redefinition;

// Phase F for ADR #1367/#1368 gives the future VM a read-only lookup boundary
// over published executable words without exposing registration or binding APIs.
#[allow(dead_code)]
mod word_lookup;

// Phase G2 for ADR #1367 adds crate-internal primitive handlers behind a
// read-only lookup boundary for VM call dispatch.
#[allow(dead_code)]
mod primitive;

// Phase G1 for ADR #1368 resolves source word names through the current word
// binding without introducing compiler or VM execution dependencies.
#[allow(dead_code)]
mod word_resolution;

// ADR #1411/#1414 keep complete source text ownership and validated spans in a
// crate-internal source-processing boundary, separate from runtime VM state.
#[allow(dead_code)]
mod source;

// ADR #1421 keeps source mappings owner-qualified by code space without adding
// source spans to runtime values, instruction operands, or VM state.
#[allow(dead_code)]
mod source_mapping;

// #1518/#1519 centralizes unpublished block construction, local branch
// completion, and source mapping order before any runtime publication step.
#[allow(dead_code)]
mod block_code;

// #1516/#1518 keeps static quotations as immutable unpublished artifacts and
// makes parent attachment an explicit local-address rebasing boundary.
#[allow(dead_code)]
mod static_quotation;

// #1504 keeps statement lowering independent from concrete instruction owners
// while preserving published-word branch patch contracts.
#[allow(dead_code)]
mod instruction_builder;

// ADR #1369/#1461 coordinates internal runtime word body construction and
// publication into one shared source-mapped published code owner.
#[allow(dead_code)]
mod published_code;

// ADR #1412 keeps non-incremental lexical analysis as a crate-internal source
// processing boundary that emits token categories and source spans only.
#[allow(dead_code)]
mod lexer;

// ADR #1413 connects complete source processing to temporary VM execution
// without adding parser, binding, public word publication, or CLI concerns.
#[allow(dead_code)]
mod source_processor;

// ADR #1484 keeps source-processing words distinct from runtime executable
// words while still allowing published source words to share ordinary binding.
#[allow(dead_code)]
mod source_word;

// ADR #1442 keeps expression operators as published primitive words resolved
// through a crate-internal lookup instead of surface name bindings.
#[allow(dead_code)]
mod operator;

// ADR #1442/#1451 parse minimal expressions into expression-local staging
// before committing resolved instructions into a temporary execution unit.
#[allow(dead_code)]
mod expression;

// ADR #1456/#1449 keep Tiny BASIC local line numbers as owner-local
// compile-time identifiers resolved by builders, never as runtime values.
#[allow(dead_code)]
mod line_number;

// ADR #1370 keeps global variable identity and storage as session-owned mutable
// state, separate from VM control state and runtime values.
#[allow(dead_code)]
mod global_variable;

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
