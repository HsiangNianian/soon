use crate::config::AppConfig;
use crate::report;

pub fn run(config: &AppConfig, json: bool) {
    let report = report::build(config);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serialize aggregate adoption report")
        );
        return;
    }

    println!("Privacy-safe adoption report");
    println!("Samples: {}", report.samples.eligible_transitions);
    if let Some(coverage) = report.samples.prediction_coverage_percent {
        println!(
            "Prediction coverage: {coverage:.1}% ({}/{})",
            report.samples.predictions, report.samples.eligible_transitions
        );
    } else {
        println!("Prediction coverage: n/a (0/0)");
    }
    println!("Suggestions shown: {}", report.suggestions.shown);
    print_outcome(
        "Accepted",
        report.suggestions.accepted,
        report.suggestions.acceptance_percent,
    );
    print_outcome(
        "Executed",
        report.suggestions.executed,
        report.suggestions.execution_percent,
    );
    print_latency("p50", report.latency_ms.p50);
    print_latency("p95", report.latency_ms.p95);
    println!("Privacy: aggregate metrics only; no command text or paths included.");
}

fn print_outcome(label: &str, count: usize, percent: Option<f64>) {
    if let Some(percent) = percent {
        println!("{label}: {count} ({percent:.1}% of shown)");
    } else {
        println!("{label}: {count} (n/a; no shown suggestions)");
    }
}

fn print_latency(label: &str, latency_ms: Option<f64>) {
    if let Some(latency_ms) = latency_ms {
        println!("{label} latency: {latency_ms:.3} ms");
    } else {
        println!("{label} latency: n/a");
    }
}
