use std::collections::HashSet;

use crate::name::NormalizedName;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MarkerIdentity {
    name: NormalizedName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerCardinality {
    One,
    Optional,
    ZeroOrMore,
    OneOrMore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkerGroup {
    markers: Vec<MarkerIdentity>,
    cardinality: MarkerCardinality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructuredGrammar {
    groups: Vec<MarkerGroup>,
    terminator: MarkerIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrammarDeclarationError {
    MissingTerminator,
    EmptyMarkerGroup { group_index: usize },
    DuplicateIntermediateMarker,
    TerminatorConflictsWithIntermediateMarker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrammarProgressError {
    UnknownMarker,
    CardinalityExceeded {
        group_index: usize,
    },
    RequiredGroupUnmet {
        required_group_index: usize,
        attempted_group_index: usize,
    },
    BackwardMarker {
        marker_group_index: usize,
        current_group_index: usize,
    },
    TerminatorBeforeRequiredGroup {
        required_group_index: usize,
    },
    AlreadyCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrammarAccept {
    Intermediate { group_index: usize },
    Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GrammarProgress {
    grammar: StructuredGrammar,
    group_index: usize,
    accepted_counts: Vec<usize>,
    completed: bool,
}

impl MarkerIdentity {
    pub(crate) fn new(name: NormalizedName) -> Self {
        Self { name }
    }

    pub(crate) fn name(&self) -> &NormalizedName {
        &self.name
    }
}

impl MarkerCardinality {
    const fn min(self) -> usize {
        match self {
            Self::One | Self::OneOrMore => 1,
            Self::Optional | Self::ZeroOrMore => 0,
        }
    }

    const fn max(self) -> Option<usize> {
        match self {
            Self::One | Self::Optional => Some(1),
            Self::ZeroOrMore | Self::OneOrMore => None,
        }
    }
}

impl MarkerGroup {
    pub(crate) fn new(markers: Vec<MarkerIdentity>, cardinality: MarkerCardinality) -> Self {
        Self {
            markers,
            cardinality,
        }
    }

    pub(crate) fn markers(&self) -> &[MarkerIdentity] {
        &self.markers
    }

    pub(crate) const fn cardinality(&self) -> MarkerCardinality {
        self.cardinality
    }

    fn contains(&self, marker: &MarkerIdentity) -> bool {
        self.markers.iter().any(|candidate| candidate == marker)
    }

    fn is_required_count_met(&self, accepted_count: usize) -> bool {
        accepted_count >= self.cardinality.min()
    }

    fn can_accept_count(&self, accepted_count: usize) -> bool {
        match self.cardinality.max() {
            Some(max) => accepted_count < max,
            None => true,
        }
    }
}

impl StructuredGrammar {
    pub(crate) fn new(
        groups: Vec<MarkerGroup>,
        terminator: Option<MarkerIdentity>,
    ) -> Result<Self, GrammarDeclarationError> {
        validate_groups(&groups)?;
        let terminator = terminator.ok_or(GrammarDeclarationError::MissingTerminator)?;
        validate_terminator(&groups, &terminator)?;

        Ok(Self { groups, terminator })
    }

    pub(crate) fn groups(&self) -> &[MarkerGroup] {
        &self.groups
    }

    pub(crate) fn terminator(&self) -> &MarkerIdentity {
        &self.terminator
    }

    pub(crate) fn start(&self) -> GrammarProgress {
        GrammarProgress {
            grammar: self.clone(),
            group_index: 0,
            accepted_counts: vec![0; self.groups.len()],
            completed: false,
        }
    }
}

impl GrammarProgress {
    pub(crate) fn accept(
        &mut self,
        marker: &MarkerIdentity,
    ) -> Result<GrammarAccept, GrammarProgressError> {
        if self.completed {
            return Err(GrammarProgressError::AlreadyCompleted);
        }

        if marker == self.grammar.terminator() {
            return self.accept_terminator();
        }

        let Some(target_group_index) = self.find_group(marker) else {
            return Err(GrammarProgressError::UnknownMarker);
        };
        self.accept_intermediate(target_group_index)
    }

    pub(crate) const fn is_completed(&self) -> bool {
        self.completed
    }

    pub(crate) fn accepted_count(&self, group_index: usize) -> Option<usize> {
        self.accepted_counts.get(group_index).copied()
    }

    fn accept_terminator(&mut self) -> Result<GrammarAccept, GrammarProgressError> {
        if let Some(required_group_index) =
            self.first_unmet_required_group(self.group_index, self.grammar.groups().len())
        {
            return Err(GrammarProgressError::TerminatorBeforeRequiredGroup {
                required_group_index,
            });
        }

        self.completed = true;
        Ok(GrammarAccept::Terminator)
    }

    fn accept_intermediate(
        &mut self,
        target_group_index: usize,
    ) -> Result<GrammarAccept, GrammarProgressError> {
        if target_group_index < self.group_index {
            return Err(GrammarProgressError::BackwardMarker {
                marker_group_index: target_group_index,
                current_group_index: self.group_index,
            });
        }

        if let Some(required_group_index) =
            self.first_unmet_required_group(self.group_index, target_group_index)
        {
            return Err(GrammarProgressError::RequiredGroupUnmet {
                required_group_index,
                attempted_group_index: target_group_index,
            });
        }

        let accepted_count = self.accepted_counts[target_group_index];
        if !self.grammar.groups()[target_group_index].can_accept_count(accepted_count) {
            return Err(GrammarProgressError::CardinalityExceeded {
                group_index: target_group_index,
            });
        }

        self.group_index = target_group_index;
        self.accepted_counts[target_group_index] += 1;
        Ok(GrammarAccept::Intermediate {
            group_index: target_group_index,
        })
    }

    fn find_group(&self, marker: &MarkerIdentity) -> Option<usize> {
        self.grammar
            .groups()
            .iter()
            .position(|group| group.contains(marker))
    }

    fn first_unmet_required_group(&self, start: usize, end: usize) -> Option<usize> {
        self.grammar.groups()[start..end]
            .iter()
            .zip(&self.accepted_counts[start..end])
            .position(|(group, accepted_count)| !group.is_required_count_met(*accepted_count))
            .map(|relative_index| start + relative_index)
    }
}

fn validate_groups(groups: &[MarkerGroup]) -> Result<(), GrammarDeclarationError> {
    let mut seen = HashSet::new();
    for (group_index, group) in groups.iter().enumerate() {
        if group.markers().is_empty() {
            return Err(GrammarDeclarationError::EmptyMarkerGroup { group_index });
        }

        for marker in group.markers() {
            if !seen.insert(marker.clone()) {
                return Err(GrammarDeclarationError::DuplicateIntermediateMarker);
            }
        }
    }

    Ok(())
}

fn validate_terminator(
    groups: &[MarkerGroup],
    terminator: &MarkerIdentity,
) -> Result<(), GrammarDeclarationError> {
    if groups.iter().any(|group| group.contains(terminator)) {
        return Err(GrammarDeclarationError::TerminatorConflictsWithIntermediateMarker);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test marker name should be valid")
    }

    fn marker(input: &str) -> MarkerIdentity {
        MarkerIdentity::new(name(input))
    }

    fn group(markers: &[&str], cardinality: MarkerCardinality) -> MarkerGroup {
        MarkerGroup::new(
            markers.iter().map(|input| marker(input)).collect(),
            cardinality,
        )
    }

    fn grammar(
        groups: Vec<MarkerGroup>,
        terminator: &str,
    ) -> Result<StructuredGrammar, GrammarDeclarationError> {
        StructuredGrammar::new(groups, Some(marker(terminator)))
    }

    #[test]
    fn grammar_without_intermediate_groups_is_valid() {
        let grammar = grammar(Vec::new(), "ENDWHILE").expect("terminator-only grammar is valid");

        assert!(grammar.groups().is_empty());
        assert_eq!(grammar.terminator().name(), &name("ENDWHILE"));
    }

    #[test]
    fn grammar_rejects_duplicate_intermediate_marker() {
        let error = grammar(
            vec![
                group(&["ELSIF"], MarkerCardinality::ZeroOrMore),
                group(&["elsif"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect_err("duplicate intermediate marker should be rejected");

        assert_eq!(error, GrammarDeclarationError::DuplicateIntermediateMarker);
    }

    #[test]
    fn grammar_rejects_terminator_that_matches_intermediate_marker() {
        let error = grammar(
            vec![group(&["ENDIF"], MarkerCardinality::Optional)],
            "endif",
        )
        .expect_err("terminator/intermediate overlap should be rejected");

        assert_eq!(
            error,
            GrammarDeclarationError::TerminatorConflictsWithIntermediateMarker
        );
    }

    #[test]
    fn grammar_rejects_missing_terminator() {
        let error =
            StructuredGrammar::new(vec![group(&["CASE"], MarkerCardinality::OneOrMore)], None)
                .expect_err("terminator is mandatory");

        assert_eq!(error, GrammarDeclarationError::MissingTerminator);
    }

    #[test]
    fn grammar_rejects_empty_marker_group() {
        let error = grammar(vec![group(&[], MarkerCardinality::Optional)], "ENDIF")
            .expect_err("empty marker group cannot classify progress");

        assert_eq!(
            error,
            GrammarDeclarationError::EmptyMarkerGroup { group_index: 0 }
        );
    }

    #[test]
    fn one_accepts_exactly_one_marker_before_progressing() {
        let grammar = grammar(
            vec![
                group(&["THEN"], MarkerCardinality::One),
                group(&["ELSE"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("ELSE")),
            Err(GrammarProgressError::RequiredGroupUnmet {
                required_group_index: 0,
                attempted_group_index: 1
            })
        );
        assert_eq!(
            progress.accept(&marker("THEN")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            progress.accept(&marker("THEN")),
            Err(GrammarProgressError::CardinalityExceeded { group_index: 0 })
        );
    }

    #[test]
    fn optional_accepts_zero_or_one_marker() {
        let grammar = grammar(
            vec![
                group(&["ELSE"], MarkerCardinality::Optional),
                group(&["FINALLY"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut skipped = grammar.start();

        assert_eq!(
            skipped.accept(&marker("FINALLY")),
            Ok(GrammarAccept::Intermediate { group_index: 1 })
        );
        assert_eq!(
            skipped.accept(&marker("ENDIF")),
            Ok(GrammarAccept::Terminator)
        );

        let mut accepted = grammar.start();
        assert_eq!(
            accepted.accept(&marker("ELSE")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            accepted.accept(&marker("ELSE")),
            Err(GrammarProgressError::CardinalityExceeded { group_index: 0 })
        );
    }

    #[test]
    fn zero_or_more_accepts_zero_one_or_many_markers() {
        let grammar = grammar(
            vec![group(&["ELSIF"], MarkerCardinality::ZeroOrMore)],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut skipped = grammar.start();
        assert_eq!(
            skipped.accept(&marker("ENDIF")),
            Ok(GrammarAccept::Terminator)
        );

        let mut accepted = grammar.start();
        assert_eq!(
            accepted.accept(&marker("ELSIF")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            accepted.accept(&marker("ELSIF")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            accepted.accept(&marker("ENDIF")),
            Ok(GrammarAccept::Terminator)
        );
    }

    #[test]
    fn one_or_more_requires_one_marker_and_accepts_many() {
        let grammar = grammar(
            vec![group(&["CASE"], MarkerCardinality::OneOrMore)],
            "ENDSWITCH",
        )
        .expect("valid grammar");
        let mut missing = grammar.start();
        assert_eq!(
            missing.accept(&marker("ENDSWITCH")),
            Err(GrammarProgressError::TerminatorBeforeRequiredGroup {
                required_group_index: 0
            })
        );

        let mut accepted = grammar.start();
        assert_eq!(
            accepted.accept(&marker("CASE")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            accepted.accept(&marker("CASE")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            accepted.accept(&marker("ENDSWITCH")),
            Ok(GrammarAccept::Terminator)
        );
    }

    #[test]
    fn minimum_zero_groups_can_be_skipped_in_sequence() {
        let grammar = grammar(
            vec![
                group(&["ELSIF"], MarkerCardinality::ZeroOrMore),
                group(&["ELSE"], MarkerCardinality::Optional),
                group(&["CLEANUP"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("CLEANUP")),
            Ok(GrammarAccept::Intermediate { group_index: 2 })
        );
    }

    #[test]
    fn required_group_blocks_later_group_and_terminator_until_satisfied() {
        let grammar = grammar(
            vec![
                group(&["CASE"], MarkerCardinality::OneOrMore),
                group(&["DEFAULT"], MarkerCardinality::Optional),
            ],
            "ENDSWITCH",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("DEFAULT")),
            Err(GrammarProgressError::RequiredGroupUnmet {
                required_group_index: 0,
                attempted_group_index: 1
            })
        );
        assert_eq!(
            progress.accept(&marker("ENDSWITCH")),
            Err(GrammarProgressError::TerminatorBeforeRequiredGroup {
                required_group_index: 0
            })
        );
        assert_eq!(
            progress.accept(&marker("CASE")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            progress.accept(&marker("ENDSWITCH")),
            Ok(GrammarAccept::Terminator)
        );
    }

    #[test]
    fn later_group_prevents_accepting_earlier_group_marker() {
        let grammar = grammar(
            vec![
                group(&["ELSIF"], MarkerCardinality::ZeroOrMore),
                group(&["ELSE"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("ELSE")),
            Ok(GrammarAccept::Intermediate { group_index: 1 })
        );
        assert_eq!(
            progress.accept(&marker("ELSIF")),
            Err(GrammarProgressError::BackwardMarker {
                marker_group_index: 0,
                current_group_index: 1
            })
        );
    }

    #[test]
    fn completed_progress_rejects_additional_markers() {
        let grammar = grammar(Vec::new(), "ENDWHILE").expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("ENDWHILE")),
            Ok(GrammarAccept::Terminator)
        );
        assert!(progress.is_completed());
        assert_eq!(
            progress.accept(&marker("ENDWHILE")),
            Err(GrammarProgressError::AlreadyCompleted)
        );
    }

    #[test]
    fn unknown_marker_is_rejected_without_progress_update() {
        let grammar = grammar(
            vec![
                group(&["ELSIF"], MarkerCardinality::ZeroOrMore),
                group(&["ELSE"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("MAYBE")),
            Err(GrammarProgressError::UnknownMarker)
        );
        assert_eq!(
            progress.accept(&marker("ELSIF")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(progress.accepted_count(0), Some(1));
    }

    #[test]
    fn while_like_grammar_uses_only_terminator() {
        let grammar = grammar(Vec::new(), "WEND").expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("WEND")),
            Ok(GrammarAccept::Terminator)
        );
    }

    #[test]
    fn if_like_grammar_uses_repeated_continuation_and_optional_fallback() {
        let grammar = grammar(
            vec![
                group(&["ELSIF"], MarkerCardinality::ZeroOrMore),
                group(&["ELSE"], MarkerCardinality::Optional),
            ],
            "ENDIF",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("ELSIF")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            progress.accept(&marker("ELSIF")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            progress.accept(&marker("ELSE")),
            Ok(GrammarAccept::Intermediate { group_index: 1 })
        );
        assert_eq!(
            progress.accept(&marker("ENDIF")),
            Ok(GrammarAccept::Terminator)
        );
    }

    #[test]
    fn switch_like_grammar_uses_required_cases_and_optional_default() {
        let grammar = grammar(
            vec![
                group(&["CASE"], MarkerCardinality::OneOrMore),
                group(&["DEFAULT"], MarkerCardinality::Optional),
            ],
            "ENDSWITCH",
        )
        .expect("valid grammar");
        let mut progress = grammar.start();

        assert_eq!(
            progress.accept(&marker("CASE")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            progress.accept(&marker("CASE")),
            Ok(GrammarAccept::Intermediate { group_index: 0 })
        );
        assert_eq!(
            progress.accept(&marker("DEFAULT")),
            Ok(GrammarAccept::Intermediate { group_index: 1 })
        );
        assert_eq!(
            progress.accept(&marker("ENDSWITCH")),
            Ok(GrammarAccept::Terminator)
        );
    }
}
