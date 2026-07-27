use crate::config::AppConfig;
use crate::{events, replay};
use serde::Serialize;

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, PartialEq, Serialize)]
pub struct AdoptionReport {
    pub schema_version: u8,
    pub samples: SampleMetrics,
    pub suggestions: SuggestionMetrics,
    pub latency_ms: LatencyMetrics,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct SampleMetrics {
    pub eligible_transitions: usize,
    pub predictions: usize,
    pub prediction_coverage_percent: Option<f64>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct SuggestionMetrics {
    pub shown: usize,
    pub accepted: usize,
    pub acceptance_percent: Option<f64>,
    pub executed: usize,
    pub execution_percent: Option<f64>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct LatencyMetrics {
    pub p50: Option<f64>,
    pub p95: Option<f64>,
}

pub fn build(config: &AppConfig) -> AdoptionReport {
    let replay = replay::run(config);
    let events = events::inspect();

    AdoptionReport {
        schema_version: SCHEMA_VERSION,
        samples: SampleMetrics {
            eligible_transitions: replay.overall.samples,
            predictions: replay.overall.covered,
            prediction_coverage_percent: percent(replay.overall.covered, replay.overall.samples),
        },
        suggestions: SuggestionMetrics {
            shown: events.shown,
            accepted: events.accepted,
            acceptance_percent: percent(events.accepted, events.shown),
            executed: events.executed,
            execution_percent: percent(events.executed, events.shown),
        },
        latency_ms: LatencyMetrics {
            p50: (replay.overall.samples > 0).then(|| replay.overall.p50_ms()),
            p95: (replay.overall.samples > 0).then(|| replay.overall.p95_ms()),
        },
    }
}

fn percent(numerator: usize, denominator: usize) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some(numerator as f64 * 100.0 / denominator as f64)
    }
}
