//! Deterministic checks over model-authored responses and runtime facts.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvaluationFinding {
    pub(crate) tool_name: String,
}

/// Detects explicit claims that a configured tool was used without that tool
/// having been dispatched by the runtime during this run.
///
/// This deliberately uses a conservative lexical heuristic rather than another
/// model call. It only considers canonical tool names supplied by the runtime
/// and requires a nearby affirmative usage phrase.
pub(crate) fn find_hallucinated_tool_invocations(
    response: &str,
    available_tools: &[String],
    executed_tools: &HashSet<String>,
) -> Vec<EvaluationFinding> {
    let mut findings = Vec::new();

    for tool_name in available_tools {
        if tool_name.is_empty() || executed_tools.contains(tool_name) {
            continue;
        }

        let mut search_from = 0;
        while let Some(relative_index) = response[search_from..].find(tool_name) {
            let start = search_from + relative_index;
            let end = start + tool_name.len();

            if claims_tool_use(response, start, end) {
                findings.push(EvaluationFinding {
                    tool_name: tool_name.clone(),
                });
                break;
            }

            search_from = end;
            if search_from >= response.len() {
                break;
            }
        }
    }

    findings
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn claims_tool_use(response: &str, tool_start: usize, tool_end: usize) -> bool {
    const CLAIM_MARKERS: &[&str] = &[
        "i used",
        "i have used",
        "i've used",
        "i called",
        "i have called",
        "i invoked",
        "i have invoked",
        "i executed",
        "i queried",
        "i ran",
        "i consulted",
        "i fetched",
        "i checked",
        "i utilized",
        "was used",
        "was called",
        "was invoked",
        "was executed",
        "he utilizado",
        "he usado",
        "utilicé",
        "utilice",
        "he llamado",
        "llamé",
        "llame",
        "he ejecutado",
        "ejecuté",
        "ejecute",
        "he consultado",
        "consulté",
        "consulte",
        "me devolvió",
        "devolvió",
        "returned",
    ];
    const NEGATIONS: &[&str] = &[
        "not",
        "never",
        "didn't",
        "did not",
        "haven't",
        "have not",
        "hasn't",
        "has not",
        "don't",
        "do not",
        "no he",
        "no ",
        "no había",
        "nunca",
        "no utilicé",
        "no utilice",
        "no llamé",
        "no llame",
    ];
    const HYPOTHETICALS: &[&str] = &[
        "could",
        "might",
        "would",
        "can",
        "should",
        "will",
        "plan to",
        "intend to",
        "podría",
        "podria",
        "puedo",
        "debería",
        "deberia",
    ];

    let window_start = floor_char_boundary(response, tool_start.saturating_sub(120));
    let window_end = ceil_char_boundary(response, (tool_end + 120).min(response.len()));
    let window = &response[window_start..window_end];
    let lower = window.to_lowercase();
    // Unicode lowercase mappings can change byte length (for example, `İ`).
    // Compute the tool offset in the lowercased window rather than reusing the
    // original UTF-8 byte offset.
    let relative_tool_start = response[window_start..tool_start].to_lowercase().len();
    let relative_tool_end = relative_tool_start + (tool_end - tool_start);

    CLAIM_MARKERS.iter().any(|marker| {
        lower.match_indices(marker).any(|(marker_start, marker_text)| {
            let marker_end = marker_start + marker_text.len();
            let distance = if marker_end <= relative_tool_start {
                relative_tool_start - marker_end
            } else if marker_start >= relative_tool_end {
                marker_start - relative_tool_end
            } else {
                0
            };

            if distance > 100 || !has_word_boundaries(&lower, marker_start, marker_end) {
                return false;
            }

            let prefix_start = floor_char_boundary(&lower, marker_start.saturating_sub(36));
            let prefix = &lower[prefix_start..marker_start];
            if NEGATIONS.iter().any(|word| prefix.contains(word)) {
                return false;
            }

            let hypothetical_start = floor_char_boundary(&lower, marker_start.saturating_sub(28));
            let hypothetical_prefix = &lower[hypothetical_start..marker_start];
            !HYPOTHETICALS
                .iter()
                .any(|word| hypothetical_prefix.contains(word))
        })
    })
}

fn has_word_boundaries(text: &str, start: usize, end: usize) -> bool {
    let before_is_word = text[..start]
        .chars()
        .next_back()
        .is_some_and(|character| character.is_alphanumeric() || character == '_');
    let after_is_word = text[end..]
        .chars()
        .next()
        .is_some_and(|character| character.is_alphanumeric() || character == '_');

    !before_is_word && !after_is_word
}

#[cfg(test)]
mod tests {
    use super::find_hallucinated_tool_invocations;
    use std::collections::HashSet;

    const TOOL: &str = "common.datetime.get_date_info";

    fn findings(response: &str, executed: &[&str]) -> Vec<super::EvaluationFinding> {
        let available = vec![TOOL.to_owned()];
        let executed = executed
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<HashSet<_>>();
        find_hallucinated_tool_invocations(response, &available, &executed)
    }

    #[test]
    fn detects_explicit_english_claim_when_tool_was_not_executed() {
        let findings = findings(
            "I used common.datetime.get_date_info to determine the weekday.",
            &[],
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tool_name, TOOL);
    }

    #[test]
    fn detects_explicit_spanish_claim_when_tool_was_not_executed() {
        let findings = findings(
            "He utilizado common.datetime.get_date_info para obtener el día de la semana.",
            &[],
        );

        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_flag_a_tool_that_was_dispatched_even_if_it_failed() {
        assert!(findings(
            "I used common.datetime.get_date_info, but the call failed.",
            &[TOOL],
        )
        .is_empty());
    }

    #[test]
    fn ignores_explicit_denials() {
        assert!(findings(
            "I did not use common.datetime.get_date_info.",
            &[],
        )
        .is_empty());
        assert!(findings(
            "No he utilizado common.datetime.get_date_info.",
            &[],
        )
        .is_empty());
    }

    #[test]
    fn ignores_hypothetical_and_non_claiming_mentions() {
        assert!(findings(
            "I could use common.datetime.get_date_info if needed.",
            &[],
        )
        .is_empty());
        assert!(findings(
            "The available tool is common.datetime.get_date_info.",
            &[],
        )
        .is_empty());
    }

    #[test]
    fn handles_unicode_before_the_tool_name() {
        assert_eq!(findings("İ He utilizado common.datetime.get_date_info.", &[]).len(), 1);
    }

    #[test]
    fn ignores_claims_about_someone_else_using_the_tool() {
        assert!(findings(
            "The user used common.datetime.get_date_info earlier.",
            &[],
        )
        .is_empty());
    }

    #[test]
    fn emits_only_one_finding_per_tool() {
        let findings = findings(
            "I used common.datetime.get_date_info. I also used common.datetime.get_date_info.",
            &[],
        );

        assert_eq!(findings.len(), 1);
    }
}
