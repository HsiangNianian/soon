use crate::config::AppConfig;
use crate::prediction::PolicyKind;
use crate::replay::{self, Metrics, Trigger};

pub fn run(config: &AppConfig) {
    let report = replay::run(config);

    println!("Local chronological replay");
    print_summary(&report.overall);
    println!();
    println!("By trigger:");
    for trigger in Trigger::ALL {
        let metrics = report.trigger(trigger);
        println!(
            "  {:<9} samples={} coverage={:.1}% top-1={:.1}% p50={:.3}ms p95={:.3}ms",
            trigger.label(),
            metrics.samples,
            metrics.coverage_percent(),
            metrics.top1_percent(),
            metrics.p50_ms(),
            metrics.p95_ms()
        );
    }
    println!();
    println!("Candidate sources:");
    for (source, metrics) in report.sources() {
        println!(
            "  {source:<21} samples={} coverage={:.1}% top-1={:.1}% p50={:.3}ms p95={:.3}ms",
            metrics.samples,
            metrics.coverage_percent(),
            metrics.top1_percent(),
            metrics.p50_ms(),
            metrics.p95_ms()
        );
    }
    println!();
    println!("Policy comparison:");
    print_policy(PolicyKind::V04Baseline, &report.baseline);
    print_policy(PolicyKind::Contextual, &report.contextual);
    println!(
        "Contextual promotion gate: {} (requires top-1 > baseline and p95 <= 20 ms)",
        if report.contextual_promotion_passes() {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!(
        "Configured policy: {}",
        crate::prediction::configured_policy(config).label()
    );
    println!();
    println!("Model outcomes:");
    println!("  Attempts: {}", report.model.attempts);
    println!(
        "  Timeout: {:.1}% ({}/{})",
        report.model.timeout_percent(),
        report.model.timeouts,
        report.model.attempts
    );
    println!(
        "  Invalid output: {:.1}% ({}/{})",
        report.model.invalid_output_percent(),
        report.model.invalid_outputs,
        report.model.attempts
    );
    println!(
        "  Deterministic fallback: {:.1}% ({}/{})",
        report.model.deterministic_fallback_percent(),
        report.model.deterministic_fallbacks,
        report.model.attempts
    );
    println!();
    println!(
        "Zsh p95 budget: {} ({:.3} ms <= {:.0} ms)",
        if report.overall.p95_ms() <= replay::ZSH_P95_BUDGET_MS {
            "PASS"
        } else {
            "FAIL"
        },
        report.overall.p95_ms(),
        replay::ZSH_P95_BUDGET_MS
    );
    println!("Privacy: aggregate metrics only; no command text printed or uploaded.");
}

fn print_policy(policy: PolicyKind, metrics: &Metrics) {
    println!(
        "  {:<21} samples={} coverage={:.1}% top-1={:.1}% p50={:.3}ms p95={:.3}ms",
        policy.label(),
        metrics.samples,
        metrics.coverage_percent(),
        metrics.top1_percent(),
        metrics.p50_ms(),
        metrics.p95_ms()
    );
}

fn print_summary(metrics: &Metrics) {
    println!("Samples: {}", metrics.samples);
    println!(
        "Coverage: {:.1}% ({}/{})",
        metrics.coverage_percent(),
        metrics.covered,
        metrics.samples
    );
    println!(
        "Top-1 match: {:.1}% ({}/{})",
        metrics.top1_percent(),
        metrics.top1_matches,
        metrics.samples
    );
    println!("p50 latency: {:.3} ms", metrics.p50_ms());
    println!("p95 latency: {:.3} ms", metrics.p95_ms());
}
