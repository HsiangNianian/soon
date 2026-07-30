use colored::Colorize;

use crate::config::AppConfig;
use crate::events;

pub fn run(config: &AppConfig, raw: bool, include_source: bool) {
    let prediction = events::predict_explicit_model(config);
    if raw {
        if let Some(prediction) = prediction {
            if include_source {
                println!(
                    "{}\t{}\t{}",
                    prediction.candidate_source,
                    prediction
                        .model_outcome
                        .map(|outcome| outcome.label())
                        .unwrap_or(""),
                    prediction.command
                );
            } else {
                println!("{}", prediction.command);
            }
        }
        return;
    }

    println!(
        "\n{}",
        "Suggested candidate (never executed):".magenta().bold()
    );
    match prediction {
        Some(prediction) => println!("  {} {}", ">".green().bold(), prediction.command.green()),
        None => println!("{}", "  No safe candidate".dimmed()),
    }
}
