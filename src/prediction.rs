use std::collections::HashMap;

use chrono::{Datelike, TimeZone, Timelike, Utc};

use crate::config::AppConfig;
use crate::events::{CommandEvent, ModelOutcome, StoredEvent, SuggestionEvent, SuggestionOutcome};
use crate::predict::{is_ignored_command, main_cmd};
use crate::privacy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyKind {
    V04Baseline,
    Contextual,
}

impl PolicyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::V04Baseline => "v0.4-baseline",
            Self::Contextual => "contextual-policy",
        }
    }

    pub fn event_source(self) -> &'static str {
        match self {
            Self::V04Baseline => "deterministic-history",
            Self::Contextual => "contextual-policy",
        }
    }
}

#[derive(Debug)]
pub struct Prediction {
    pub command: String,
    pub policy: PolicyKind,
    pub candidate_source: &'static str,
    pub signal_groups: Vec<&'static str>,
    pub model_outcome: Option<ModelOutcome>,
}

#[derive(Debug, Clone)]
pub struct ExternalCandidate {
    pub command: String,
    pub source: &'static str,
    pub rank: usize,
    pub candidate_count: usize,
}

#[derive(Default)]
pub struct Memory<'a> {
    pub commands: Vec<&'a CommandEvent>,
    pub suggestions: Vec<&'a SuggestionEvent>,
}

impl<'a> Memory<'a> {
    pub(crate) fn from_stored(events: &'a [StoredEvent]) -> Self {
        let mut memory = Self::default();
        for event in events {
            memory.push(event);
        }
        memory
    }

    pub(crate) fn push(&mut self, event: &'a StoredEvent) {
        match event {
            StoredEvent::Command(command) => self.commands.push(command),
            StoredEvent::Suggestion(suggestion) => self.suggestions.push(suggestion),
        }
    }
}

struct Candidate<'a> {
    command: String,
    source: &'static str,
    observations: Vec<Observation<'a>>,
    model_rank: Option<(usize, usize)>,
}

struct Observation<'a> {
    event: &'a CommandEvent,
    previous: Option<&'a CommandEvent>,
    index: usize,
}

trait CandidateSource {
    fn label(&self) -> &'static str;

    fn retrieve<'a>(
        &self,
        current: &CommandEvent,
        memory: &Memory<'a>,
        config: &AppConfig,
    ) -> Vec<Candidate<'a>>;
}

trait Ranker {
    fn rank(
        &self,
        current: &CommandEvent,
        candidates: Vec<Candidate<'_>>,
        memory: &Memory<'_>,
    ) -> Option<Prediction>;
}

struct TransitionHistorySource;

impl CandidateSource for TransitionHistorySource {
    fn label(&self) -> &'static str {
        "event-history"
    }

    fn retrieve<'a>(
        &self,
        current: &CommandEvent,
        memory: &Memory<'a>,
        config: &AppConfig,
    ) -> Vec<Candidate<'a>> {
        let by_id: HashMap<&str, &CommandEvent> = memory
            .commands
            .iter()
            .map(|event| (event.id.as_str(), *event))
            .collect();
        let wanted_success = current.exit_code.map(|code| code == 0);
        let mut candidates = HashMap::<&str, Candidate<'a>>::new();

        for (index, next) in memory.commands.iter().copied().enumerate() {
            let Some(previous) = next
                .previous_event_id
                .as_deref()
                .and_then(|id| by_id.get(id).copied())
            else {
                continue;
            };
            if previous.command.trim() != current.command.trim()
                || matches!(
                    (previous.exit_code.map(|code| code == 0), wanted_success),
                    (Some(observed), Some(wanted)) if observed != wanted
                )
                || next.command.chars().any(char::is_control)
                || privacy::rejection_reason(&next.command, config).is_some()
                || is_ignored_command(main_cmd(&next.command), config)
            {
                continue;
            }

            candidates
                .entry(next.command.trim())
                .or_insert_with(|| Candidate {
                    command: next.command.trim().to_string(),
                    source: self.label(),
                    observations: Vec::new(),
                    model_rank: None,
                })
                .observations
                .push(Observation {
                    event: next,
                    previous: Some(previous),
                    index,
                });
        }

        candidates.into_values().collect()
    }
}

#[derive(Default)]
struct CandidateScore {
    result_matches: usize,
    evidence: usize,
    directory_matches: usize,
    latest_index: usize,
}

struct V04Ranker;

impl Ranker for V04Ranker {
    fn rank(
        &self,
        current: &CommandEvent,
        candidates: Vec<Candidate<'_>>,
        _memory: &Memory<'_>,
    ) -> Option<Prediction> {
        let mut ranked: Vec<_> = candidates
            .into_iter()
            .map(|candidate| {
                let score = candidate.observations.iter().fold(
                    CandidateScore::default(),
                    |mut score, observation| {
                        score.result_matches += usize::from(
                            observation
                                .previous
                                .is_some_and(|event| event.exit_code.is_some()),
                        );
                        score.evidence += 1;
                        score.directory_matches += usize::from(
                            current.cwd.is_some()
                                && observation
                                    .previous
                                    .is_some_and(|event| event.cwd == current.cwd),
                        );
                        score.latest_index = score.latest_index.max(observation.index);
                        score
                    },
                );
                (candidate, score)
            })
            .collect();
        ranked.sort_by(|(candidate_a, score_a), (candidate_b, score_b)| {
            score_b
                .result_matches
                .cmp(&score_a.result_matches)
                .then_with(|| score_b.directory_matches.cmp(&score_a.directory_matches))
                .then_with(|| score_b.evidence.cmp(&score_a.evidence))
                .then_with(|| score_b.latest_index.cmp(&score_a.latest_index))
                .then_with(|| candidate_a.command.cmp(&candidate_b.command))
        });
        ranked.into_iter().next().map(|(candidate, _)| Prediction {
            command: candidate.command.to_string(),
            policy: PolicyKind::V04Baseline,
            candidate_source: candidate.source,
            signal_groups: v04_signal_groups(current, &candidate),
            model_outcome: None,
        })
    }
}

fn v04_signal_groups(current: &CommandEvent, candidate: &Candidate<'_>) -> Vec<&'static str> {
    let mut groups = Vec::new();
    if candidate.observations.iter().any(|observation| {
        observation
            .previous
            .is_some_and(|event| event.exit_code.is_some())
    }) {
        groups.push("result");
    }
    if current.cwd.is_some()
        && candidate.observations.iter().any(|observation| {
            observation
                .previous
                .is_some_and(|event| event.cwd.is_some())
        })
    {
        groups.push("directory");
    }
    groups.extend(["frequency", "recency"]);
    groups
}

struct FullHistorySource;

impl CandidateSource for FullHistorySource {
    fn label(&self) -> &'static str {
        "event-history"
    }

    fn retrieve<'a>(
        &self,
        current: &CommandEvent,
        memory: &Memory<'a>,
        config: &AppConfig,
    ) -> Vec<Candidate<'a>> {
        let by_id: HashMap<&str, &CommandEvent> = memory
            .commands
            .iter()
            .map(|event| (event.id.as_str(), *event))
            .collect();
        let mut candidates = HashMap::<&str, Candidate<'a>>::new();

        for (index, event) in memory.commands.iter().copied().enumerate() {
            if event.id == current.id
                || event.command.chars().any(char::is_control)
                || privacy::rejection_reason(&event.command, config).is_some()
                || is_ignored_command(main_cmd(&event.command), config)
            {
                continue;
            }
            let previous = event
                .previous_event_id
                .as_deref()
                .and_then(|id| by_id.get(id).copied());
            candidates
                .entry(event.command.trim())
                .or_insert_with(|| Candidate {
                    command: event.command.trim().to_string(),
                    source: self.label(),
                    observations: Vec::new(),
                    model_rank: None,
                })
                .observations
                .push(Observation {
                    event,
                    previous,
                    index,
                });
        }

        candidates.into_values().collect()
    }
}

#[derive(Default)]
struct ContextualScore {
    total: f64,
    second_order_matches: usize,
    second_order_known: usize,
    directory_matches: usize,
    directory_known: usize,
    repository_matches: usize,
    repository_known: usize,
    branch_matches: usize,
    branch_known: usize,
    hour_matches: usize,
    hour_known: usize,
    weekday_matches: usize,
    weekday_known: usize,
    result_matches: usize,
    result_known: usize,
    duration_matches: usize,
    duration_known: usize,
    first_order_matches: usize,
    first_order_known: usize,
    feedback: usize,
    model_support: bool,
    evidence: usize,
    latest_index: usize,
}

struct ContextualRanker;

impl Ranker for ContextualRanker {
    fn rank(
        &self,
        current: &CommandEvent,
        mut candidates: Vec<Candidate<'_>>,
        memory: &Memory<'_>,
    ) -> Option<Prediction> {
        let by_id: HashMap<&str, &CommandEvent> = memory
            .commands
            .iter()
            .map(|event| (event.id.as_str(), *event))
            .collect();
        let current_predecessor_command = current
            .previous_event_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied())
            .map(|event| event.command.trim());
        let has_first_order_evidence = candidates.iter().any(|candidate| {
            candidate.observations.iter().any(|observation| {
                observation
                    .previous
                    .is_some_and(|event| event.command.trim() == current.command.trim())
            })
        });
        if has_first_order_evidence {
            candidates.retain(|candidate| {
                candidate.model_rank.is_some()
                    || candidate.observations.iter().any(|observation| {
                        observation
                            .previous
                            .is_some_and(|event| event.command.trim() == current.command.trim())
                    })
            });
        } else {
            candidates.retain(|candidate| candidate.command != current.command.trim());
        }

        let wanted_success = current.exit_code.map(|code| code == 0);
        let candidate_count = candidates.len();
        let total_evidence: usize = candidates
            .iter()
            .map(|candidate| candidate.observations.len())
            .sum();
        let max_index = memory.commands.len().saturating_sub(1).max(1);
        let mut ranked: Vec<_> = candidates
            .into_iter()
            .map(|candidate| {
                let score = candidate.observations.iter().fold(
                    ContextualScore::default(),
                    |mut score, observation| {
                        score.second_order_known += usize::from(
                            current_predecessor_command.is_some()
                                && observation
                                    .previous
                                    .and_then(|event| event.previous_event_id.as_deref())
                                    .and_then(|id| by_id.get(id).copied())
                                    .is_some(),
                        );
                        score.second_order_matches += usize::from(matches!(
                            (
                                current_predecessor_command,
                                observation
                                    .previous
                                    .and_then(|event| event.previous_event_id.as_deref())
                                    .and_then(|id| by_id.get(id).copied())
                                    .map(|event| event.command.trim())
                            ),
                            (Some(current), Some(observed)) if current == observed
                        ));
                        score.directory_known +=
                            usize::from(current.cwd.is_some() && observation.event.cwd.is_some());
                        score.directory_matches += usize::from(
                            current.cwd.is_some() && observation.event.cwd == current.cwd,
                        );
                        score.repository_known += usize::from(
                            current.repository.is_some() && observation.event.repository.is_some(),
                        );
                        score.repository_matches += usize::from(
                            current.repository.is_some()
                                && observation.event.repository == current.repository,
                        );
                        score.branch_known += usize::from(
                            current.branch.is_some() && observation.event.branch.is_some(),
                        );
                        score.branch_matches += usize::from(
                            current.branch.is_some() && observation.event.branch == current.branch,
                        );
                        score.hour_known += usize::from(
                            current.started_at_ms.and_then(hour_bucket).is_some()
                                && observation
                                    .event
                                    .started_at_ms
                                    .and_then(hour_bucket)
                                    .is_some(),
                        );
                        score.hour_matches += usize::from(matches!(
                            (
                                current.started_at_ms.and_then(hour_bucket),
                                observation.event.started_at_ms.and_then(hour_bucket)
                            ),
                            (Some(current), Some(observed)) if current == observed
                        ));
                        score.weekday_known += usize::from(
                            current.started_at_ms.and_then(weekday).is_some()
                                && observation.event.started_at_ms.and_then(weekday).is_some(),
                        );
                        score.weekday_matches += usize::from(matches!(
                            (
                                current.started_at_ms.and_then(weekday),
                                observation.event.started_at_ms.and_then(weekday)
                            ),
                            (Some(current), Some(observed)) if current == observed
                        ));
                        score.result_known += usize::from(
                            wanted_success.is_some()
                                && observation
                                    .previous
                                    .is_some_and(|event| event.exit_code.is_some()),
                        );
                        score.result_matches +=
                            usize::from(observation.previous.is_some_and(|event| {
                                matches!(
                                    (event.exit_code.map(|code| code == 0), wanted_success),
                                    (Some(observed), Some(wanted)) if observed == wanted
                                )
                            }));
                        score.duration_known += usize::from(
                            current.duration_ms.is_some()
                                && observation
                                    .previous
                                    .is_some_and(|event| event.duration_ms.is_some()),
                        );
                        score.duration_matches += usize::from(matches!(
                            (
                                current.duration_ms.map(duration_bucket),
                                observation
                                    .previous
                                    .and_then(|event| event.duration_ms)
                                    .map(duration_bucket)
                            ),
                            (Some(current), Some(observed)) if current == observed
                        ));
                        score.first_order_known += usize::from(observation.previous.is_some());
                        score.first_order_matches +=
                            usize::from(observation.previous.is_some_and(|event| {
                                event.command.trim() == current.command.trim()
                            }));
                        score.evidence += 1;
                        score.latest_index = score.latest_index.max(observation.index);
                        score
                    },
                );
                let feedback = memory
                    .suggestions
                    .iter()
                    .filter(|event| event.command.trim() == candidate.command.as_str())
                    .map(|event| match event.outcome {
                        SuggestionOutcome::Accepted => 2,
                        SuggestionOutcome::Executed => 4,
                        SuggestionOutcome::Shown | SuggestionOutcome::Dismissed => 0,
                    })
                    .sum();
                let model_support = candidate.model_rank.is_some();
                let mut score = ContextualScore {
                    feedback,
                    model_support,
                    ..score
                };
                score.total = smoothed_prior(score.evidence, total_evidence, candidate_count)
                    + 2.0 * smoothed_match(score.first_order_matches, score.first_order_known, 2)
                    + 2.5 * smoothed_match(score.second_order_matches, score.second_order_known, 2)
                    + 1.5 * smoothed_match(score.directory_matches, score.directory_known, 2)
                    + 1.25 * smoothed_match(score.repository_matches, score.repository_known, 2)
                    + 1.25 * smoothed_match(score.branch_matches, score.branch_known, 2)
                    + 0.75 * smoothed_match(score.hour_matches, score.hour_known, 6)
                    + 0.75 * smoothed_match(score.weekday_matches, score.weekday_known, 7)
                    + 1.5 * smoothed_match(score.result_matches, score.result_known, 2)
                    + 0.75 * smoothed_match(score.duration_matches, score.duration_known, 4)
                    + 1.5 * (score.feedback as f64).ln_1p()
                    + candidate
                        .model_rank
                        .map(|(rank, total)| {
                            let preference = total.saturating_sub(rank).max(1) as f64;
                            8.0 * preference.ln_1p()
                        })
                        .unwrap_or(0.0)
                    + 0.25 * score.latest_index as f64 / max_index as f64;
                (candidate, score)
            })
            .collect();
        ranked.sort_by(|(candidate_a, score_a), (candidate_b, score_b)| {
            score_b
                .total
                .total_cmp(&score_a.total)
                .then_with(|| score_b.evidence.cmp(&score_a.evidence))
                .then_with(|| score_b.latest_index.cmp(&score_a.latest_index))
                .then_with(|| candidate_a.command.cmp(&candidate_b.command))
        });
        ranked
            .into_iter()
            .next()
            .map(|(candidate, score)| Prediction {
                command: candidate.command.to_string(),
                policy: PolicyKind::Contextual,
                candidate_source: candidate.source,
                signal_groups: contextual_signal_groups(current, &candidate, &score),
                model_outcome: None,
            })
    }
}

fn contextual_signal_groups(
    current: &CommandEvent,
    candidate: &Candidate<'_>,
    score: &ContextualScore,
) -> Vec<&'static str> {
    let mut groups = Vec::new();
    if score.first_order_matches > 0 || score.second_order_matches > 0 {
        groups.push("transition");
    }
    if current.cwd.is_some()
        && candidate
            .observations
            .iter()
            .any(|observation| observation.event.cwd.is_some())
    {
        groups.push("directory");
    }
    if current.repository.is_some()
        && candidate
            .observations
            .iter()
            .any(|observation| observation.event.repository.is_some())
    {
        groups.push("repository");
    }
    if current.branch.is_some()
        && candidate
            .observations
            .iter()
            .any(|observation| observation.event.branch.is_some())
    {
        groups.push("branch");
    }
    if current.started_at_ms.is_some()
        && candidate
            .observations
            .iter()
            .any(|observation| observation.event.started_at_ms.is_some())
    {
        groups.push("time");
    }
    if current.exit_code.is_some()
        && candidate.observations.iter().any(|observation| {
            observation
                .previous
                .is_some_and(|event| event.exit_code.is_some())
        })
    {
        groups.push("result");
    }
    if current.duration_ms.is_some()
        && candidate.observations.iter().any(|observation| {
            observation
                .previous
                .is_some_and(|event| event.duration_ms.is_some())
        })
    {
        groups.push("duration");
    }
    if score.feedback > 0 {
        groups.push("feedback");
    }
    if score.model_support {
        groups.push("model");
    }
    groups.extend(["frequency", "recency"]);
    groups
}

fn hour_bucket(timestamp_ms: i64) -> Option<u32> {
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|timestamp| timestamp.hour() / 4)
}

fn weekday(timestamp_ms: i64) -> Option<u32> {
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|timestamp| timestamp.weekday().num_days_from_monday())
}

fn duration_bucket(duration_ms: u64) -> u8 {
    match duration_ms {
        0..=999 => 0,
        1_000..=9_999 => 1,
        10_000..=59_999 => 2,
        _ => 3,
    }
}

fn smoothed_prior(evidence: usize, total_evidence: usize, candidate_count: usize) -> f64 {
    if candidate_count == 0 {
        return 0.0;
    }
    ((evidence + 1) as f64 / (total_evidence + candidate_count) as f64).ln()
}

fn smoothed_match(matches: usize, known: usize, categories: usize) -> f64 {
    if known == 0 {
        return 0.0;
    }
    (((matches + 1) * categories) as f64 / (known + categories) as f64).ln()
}

pub fn predict(
    policy: PolicyKind,
    current: &CommandEvent,
    memory: &Memory<'_>,
    config: &AppConfig,
) -> Option<Prediction> {
    match policy {
        PolicyKind::V04Baseline => {
            let candidates = TransitionHistorySource.retrieve(current, memory, config);
            V04Ranker.rank(current, candidates, memory)
        }
        PolicyKind::Contextual => {
            let candidates = FullHistorySource.retrieve(current, memory, config);
            ContextualRanker.rank(current, candidates, memory)
        }
    }
}

pub fn predict_with_external(
    current: &CommandEvent,
    memory: &Memory<'_>,
    external: &[ExternalCandidate],
    config: &AppConfig,
) -> Option<Prediction> {
    let mut candidates = FullHistorySource.retrieve(current, memory, config);
    for model_candidate in external {
        if privacy::model_candidate_rejection_reason(&model_candidate.command, config).is_some()
            || is_ignored_command(main_cmd(&model_candidate.command), config)
        {
            continue;
        }
        if let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.command == model_candidate.command.trim())
        {
            candidate.source = model_candidate.source;
            candidate.model_rank = Some((model_candidate.rank, model_candidate.candidate_count));
        } else {
            candidates.push(Candidate {
                command: model_candidate.command.trim().to_string(),
                source: model_candidate.source,
                observations: Vec::new(),
                model_rank: Some((model_candidate.rank, model_candidate.candidate_count)),
            });
        }
    }
    ContextualRanker.rank(current, candidates, memory)
}

pub fn local_candidate_shortlist(
    current: &CommandEvent,
    memory: &Memory<'_>,
    config: &AppConfig,
    limit: usize,
) -> Vec<String> {
    let mut candidates = FullHistorySource.retrieve(current, memory, config);
    let has_transition = candidates.iter().any(|candidate| {
        candidate.observations.iter().any(|observation| {
            observation
                .previous
                .is_some_and(|event| event.command.trim() == current.command.trim())
        })
    });
    if has_transition {
        candidates.retain(|candidate| {
            candidate.observations.iter().any(|observation| {
                observation
                    .previous
                    .is_some_and(|event| event.command.trim() == current.command.trim())
            })
        });
    }
    candidates.sort_by(|a, b| {
        b.observations
            .len()
            .cmp(&a.observations.len())
            .then_with(|| {
                b.observations
                    .iter()
                    .map(|observation| observation.index)
                    .max()
                    .cmp(
                        &a.observations
                            .iter()
                            .map(|observation| observation.index)
                            .max(),
                    )
            })
            .then_with(|| a.command.cmp(&b.command))
    });
    candidates
        .into_iter()
        .map(|candidate| candidate.command)
        .take(limit)
        .collect()
}

pub fn configured_policy(config: &AppConfig) -> PolicyKind {
    if config.prediction.policy == "contextual" {
        PolicyKind::Contextual
    } else {
        PolicyKind::V04Baseline
    }
}
