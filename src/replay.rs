use std::collections::HashMap;
use std::time::Instant;

use crate::config::AppConfig;
use crate::events::{
    self, CommandEvent, ModelOutcome, StoredEvent, SuggestionEvent, SuggestionOutcome,
};
use crate::predict::{is_ignored_command, main_cmd};
use crate::privacy;

pub const ZSH_P95_BUDGET_MS: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trigger {
    Manual,
    NextStep,
    Repair,
}

impl Trigger {
    pub const ALL: [Self; 3] = [Self::Manual, Self::NextStep, Self::Repair];

    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::NextStep => "next-step",
            Self::Repair => "repair",
        }
    }

    fn from_exit_code(exit_code: Option<i32>) -> Self {
        match exit_code {
            None => Self::Manual,
            Some(0) => Self::NextStep,
            Some(_) => Self::Repair,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub samples: usize,
    pub covered: usize,
    pub top1_matches: usize,
    latencies_ms: Vec<f64>,
}

impl Metrics {
    pub fn coverage_percent(&self) -> f64 {
        percent(self.covered, self.samples)
    }

    pub fn top1_percent(&self) -> f64 {
        percent(self.top1_matches, self.samples)
    }

    pub fn p50_ms(&self) -> f64 {
        percentile(&self.latencies_ms, 0.50)
    }

    pub fn p95_ms(&self) -> f64 {
        percentile(&self.latencies_ms, 0.95)
    }

    fn record(&mut self, prediction: Option<&str>, actual: &str, latency_ms: f64) {
        self.samples += 1;
        self.covered += usize::from(prediction.is_some());
        self.top1_matches += usize::from(prediction == Some(actual));
        self.latencies_ms.push(latency_ms);
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelMetrics {
    pub attempts: usize,
    pub timeouts: usize,
    pub invalid_outputs: usize,
    pub deterministic_fallbacks: usize,
}

impl ModelMetrics {
    pub fn timeout_percent(&self) -> f64 {
        percent(self.timeouts, self.attempts)
    }

    pub fn invalid_output_percent(&self) -> f64 {
        percent(self.invalid_outputs, self.attempts)
    }

    pub fn deterministic_fallback_percent(&self) -> f64 {
        percent(self.deterministic_fallbacks, self.attempts)
    }

    fn record(&mut self, outcome: ModelOutcome) {
        self.attempts += 1;
        match outcome {
            ModelOutcome::Success => {}
            ModelOutcome::Timeout => self.timeouts += 1,
            ModelOutcome::InvalidOutput => self.invalid_outputs += 1,
            ModelOutcome::DeterministicFallback => self.deterministic_fallbacks += 1,
        }
    }
}

#[derive(Debug, Default)]
pub struct ReplayReport {
    pub overall: Metrics,
    by_trigger: HashMap<Trigger, Metrics>,
    by_source: HashMap<String, Metrics>,
    pub model: ModelMetrics,
}

impl ReplayReport {
    pub fn trigger(&self, trigger: Trigger) -> Metrics {
        self.by_trigger.get(&trigger).cloned().unwrap_or_default()
    }

    pub fn sources(&self) -> Vec<(&str, Metrics)> {
        let mut sources: Vec<_> = self
            .by_source
            .iter()
            .map(|(source, metrics)| (source.as_str(), metrics.clone()))
            .collect();
        sources.sort_by_key(|(source, _)| *source);
        sources
    }
}

#[derive(Debug)]
struct Transition<'a> {
    previous: &'a CommandEvent,
    next: &'a CommandEvent,
    index: usize,
}

#[derive(Debug, Default)]
struct CandidateScore {
    result_matches: usize,
    evidence: usize,
    directory_matches: usize,
    latest_index: usize,
}

#[derive(Debug)]
struct RecordedSuggestion<'a> {
    source: &'static str,
    event: &'a SuggestionEvent,
}

pub fn run(config: &AppConfig) -> ReplayReport {
    let stored_events = events::replay_events();
    let mut seen = HashMap::<&str, &CommandEvent>::new();
    let mut transitions = Vec::<Transition<'_>>::new();
    let mut pending = HashMap::<&str, Vec<RecordedSuggestion<'_>>>::new();
    let mut pending_manual = Vec::<RecordedSuggestion<'_>>::new();
    let mut report = ReplayReport::default();

    for stored_event in &stored_events {
        match stored_event {
            StoredEvent::Command(command) if is_safe(command, config) => {
                if is_candidate(&command.command, config) {
                    score_recorded(&mut report, std::mem::take(&mut pending_manual), command);
                } else {
                    pending_manual.clear();
                }

                if let Some(previous_id) = command.previous_event_id.as_deref() {
                    if let Some(previous) = seen.get(previous_id).copied() {
                        if is_candidate(&command.command, config) {
                            let started = Instant::now();
                            let prediction = predict(previous, &transitions, config);
                            let latency_ms = started.elapsed().as_secs_f64() * 1_000.0;
                            let trigger = Trigger::from_exit_code(previous.exit_code);

                            report.overall.record(
                                prediction.as_deref(),
                                command.command.trim(),
                                latency_ms,
                            );
                            report.by_trigger.entry(trigger).or_default().record(
                                prediction.as_deref(),
                                command.command.trim(),
                                latency_ms,
                            );
                            report
                                .by_source
                                .entry("deterministic-history".to_string())
                                .or_default()
                                .record(prediction.as_deref(), command.command.trim(), latency_ms);
                        }

                        if let Some(suggestions) = pending.remove(previous.id.as_str()) {
                            if is_candidate(&command.command, config) {
                                score_recorded(&mut report, suggestions, command);
                            }
                        }
                        transitions.push(Transition {
                            previous,
                            next: command,
                            index: transitions.len(),
                        });
                    }
                }
                seen.insert(command.id.as_str(), command);
            }
            StoredEvent::Suggestion(suggestion)
                if suggestion.outcome == SuggestionOutcome::Shown
                    && is_safe_suggestion(suggestion, config) =>
            {
                let recorded = RecordedSuggestion {
                    source: canonical_source(&suggestion.candidate_source),
                    event: suggestion,
                };
                if let Some(command_event_id) = suggestion.command_event_id.as_deref() {
                    pending.entry(command_event_id).or_default().push(recorded);
                } else {
                    pending_manual.push(recorded);
                }
            }
            StoredEvent::Command(_) | StoredEvent::Suggestion(_) => {}
        }
    }

    report
}

fn score_recorded(
    report: &mut ReplayReport,
    suggestions: Vec<RecordedSuggestion<'_>>,
    actual: &CommandEvent,
) {
    for suggestion in suggestions {
        report
            .by_source
            .entry(suggestion.source.to_string())
            .or_default()
            .record(
                Some(suggestion.event.command.trim()),
                actual.command.trim(),
                suggestion.event.latency_ms,
            );
        if let Some(outcome) = suggestion.event.model_outcome {
            report.model.record(outcome);
        }
    }
}

fn is_safe(event: &CommandEvent, config: &AppConfig) -> bool {
    privacy::rejection_reason(&event.command, config).is_none()
        && !event.command.chars().any(char::is_control)
}

fn is_safe_suggestion(event: &SuggestionEvent, config: &AppConfig) -> bool {
    privacy::rejection_reason(&event.command, config).is_none()
        && !event.command.chars().any(char::is_control)
        && is_candidate(&event.command, config)
}

fn is_candidate(command: &str, config: &AppConfig) -> bool {
    !is_ignored_command(main_cmd(command), config)
}

fn canonical_source(source: &str) -> &'static str {
    match source
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .as_str()
    {
        "history" | "deterministic-history" => "deterministic-history",
        "context" | "contextual-policy" => "contextual-policy",
        "local-model" => "local-model",
        "remote" | "remote-provider" => "remote-provider",
        "deterministic-fallback" => "deterministic-fallback",
        _ => "other",
    }
}

fn predict(
    current: &CommandEvent,
    transitions: &[Transition<'_>],
    config: &AppConfig,
) -> Option<String> {
    let wanted_success = current.exit_code.map(|code| code == 0);
    let mut candidates = HashMap::<&str, CandidateScore>::new();

    for transition in transitions {
        if transition.previous.command.trim() != current.command.trim()
            || matches!(
                (
                    transition.previous.exit_code.map(|code| code == 0),
                    wanted_success
                ),
                (Some(observed), Some(wanted)) if observed != wanted
            )
            || privacy::rejection_reason(&transition.next.command, config).is_some()
            || is_ignored_command(main_cmd(&transition.next.command), config)
        {
            continue;
        }

        let score = candidates
            .entry(transition.next.command.trim())
            .or_default();
        score.result_matches += usize::from(transition.previous.exit_code.is_some());
        score.evidence += 1;
        score.directory_matches +=
            usize::from(current.cwd.is_some() && transition.previous.cwd == current.cwd);
        score.latest_index = transition.index;
    }

    let mut ranked: Vec<_> = candidates.into_iter().collect();
    ranked.sort_by(|(command_a, score_a), (command_b, score_b)| {
        score_b
            .result_matches
            .cmp(&score_a.result_matches)
            .then_with(|| score_b.directory_matches.cmp(&score_a.directory_matches))
            .then_with(|| score_b.evidence.cmp(&score_a.evidence))
            .then_with(|| score_b.latest_index.cmp(&score_a.latest_index))
            .then_with(|| command_a.cmp(command_b))
    });
    ranked
        .into_iter()
        .next()
        .map(|(command, _)| command.to_string())
}

fn percent(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}
