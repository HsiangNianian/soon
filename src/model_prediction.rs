use std::collections::HashSet;
use std::time::Duration;

use crate::config::AppConfig;
use crate::events::{CommandEvent, ModelOutcome};
use crate::predict::{is_ignored_command, main_cmd};
use crate::prediction::{self, ExternalCandidate, Memory, Prediction};
use crate::privacy;

const MAX_CONTEXT_COMMANDS: usize = 6;
const MAX_CANDIDATES: usize = 5;
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rerank,
    Repair,
    Generate,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Rerank => "rerank",
            Self::Repair => "repair",
            Self::Generate => "generate",
        }
    }
}

#[derive(Debug)]
enum AttemptError {
    Timeout,
    InvalidOutput,
    Unavailable,
}

pub fn configured_mode(config: &AppConfig, exit_code: Option<i32>) -> Option<Mode> {
    match config.prediction.model_mode.as_str() {
        "rerank" => Some(Mode::Rerank),
        "repair" if exit_code.is_some_and(|code| code != 0) => Some(Mode::Repair),
        "rerank-repair" if exit_code.is_some_and(|code| code != 0) => Some(Mode::Repair),
        "rerank-repair" => Some(Mode::Rerank),
        _ => None,
    }
}

pub fn augment_if_configured(
    current: &CommandEvent,
    memory: &Memory<'_>,
    deterministic: Option<Prediction>,
    config: &AppConfig,
) -> Option<Prediction> {
    let Some(mode) = configured_mode(config, current.exit_code) else {
        return deterministic;
    };
    augment(current, memory, deterministic, mode, config)
}

pub fn augment(
    current: &CommandEvent,
    memory: &Memory<'_>,
    deterministic: Option<Prediction>,
    mode: Mode,
    config: &AppConfig,
) -> Option<Prediction> {
    let local_candidates =
        prediction::local_candidate_shortlist(current, memory, config, MAX_CANDIDATES);
    match request_candidates(mode, current, memory, &local_candidates, config) {
        Ok(external) => {
            if let Some(mut selected) =
                prediction::predict_with_external(current, memory, &external, config)
            {
                selected.model_outcome = Some(ModelOutcome::Success);
                Some(selected)
            } else {
                fallback(deterministic, ModelOutcome::InvalidOutput)
            }
        }
        Err(AttemptError::Timeout) => fallback(deterministic, ModelOutcome::Timeout),
        Err(AttemptError::InvalidOutput) => fallback(deterministic, ModelOutcome::InvalidOutput),
        Err(AttemptError::Unavailable) => {
            fallback(deterministic, ModelOutcome::DeterministicFallback)
        }
    }
}

fn fallback(deterministic: Option<Prediction>, model_outcome: ModelOutcome) -> Option<Prediction> {
    deterministic.map(|mut prediction| {
        prediction.candidate_source = "deterministic-fallback";
        prediction.model_outcome = Some(model_outcome);
        prediction
    })
}

fn request_candidates(
    mode: Mode,
    current: &CommandEvent,
    memory: &Memory<'_>,
    local_candidates: &[String],
    config: &AppConfig,
) -> Result<Vec<ExternalCandidate>, AttemptError> {
    if config.llm.api_url.trim().is_empty() || config.llm.model.trim().is_empty() {
        return Err(AttemptError::Unavailable);
    }
    if mode == Mode::Rerank && local_candidates.is_empty() {
        return Err(AttemptError::Unavailable);
    }

    let recent_commands = filtered_recent_commands(memory, config);
    let current_command = (privacy::rejection_reason(&current.command, config).is_none()
        && !current.command.chars().any(char::is_control)
        && !current.command.trim().is_empty())
    .then(|| current.command.trim().to_string());
    let result = current
        .exit_code
        .map(|code| if code == 0 { "success" } else { "failure" });
    let context = serde_json::json!({
        "mode": mode.label(),
        "current_command": current_command,
        "previous_result": result,
        "recent_commands": recent_commands,
        "local_candidates": local_candidates,
    });
    let (system_prompt, user_prompt) = match mode {
        Mode::Rerank => (
            "You rerank shell command text but never execute commands. Return strict JSON only."
                .to_string(),
            format!(
                "Return only a JSON object with a commands array. Reorder only commands from local_candidates.\nContext:\n{context}"
            ),
        ),
        Mode::Repair => (
            "You repair failed shell commands. Never repeat the failed command. Correct obvious misspellings or flag errors. Return strict JSON only."
                .to_string(),
            format!(
                "The command failed. Return only {{\"commands\":[\"corrected command\"]}}. Failed command: {}\nFiltered local candidates: {}",
                current_command.as_deref().unwrap_or("unknown"),
                serde_json::to_string(local_candidates)
                    .map_err(|_| AttemptError::InvalidOutput)?
            ),
        ),
        Mode::Generate => (
            "You suggest shell command text but never execute commands. Return strict JSON only."
                .to_string(),
            format!(
                "Return only a JSON object with a commands array containing likely next shell commands.\nContext:\n{context}"
            ),
        ),
    };
    let request_body = serde_json::json!({
        "model": config.llm.model,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_prompt
            }
        ],
        "temperature": 0.0,
        "max_tokens": 256
    });

    let agent_config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(
            config.prediction.model_timeout_ms,
        )))
        .build();
    let agent: ureq::Agent = agent_config.into();
    let mut request = agent
        .post(&provider_url(&config.llm.api_url))
        .header("Content-Type", "application/json");
    if let Ok(api_key) = std::env::var(&config.llm.api_key_env) {
        if !api_key.is_empty() {
            request = request.header("Authorization", &format!("Bearer {api_key}"));
        }
    }
    let encoded = serde_json::to_vec(&request_body).map_err(|_| AttemptError::InvalidOutput)?;
    let mut response = request
        .send(encoded.as_slice())
        .map_err(|error| match error {
            ureq::Error::Timeout(_) => AttemptError::Timeout,
            _ => AttemptError::Unavailable,
        })?;
    let response_text = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|_| AttemptError::InvalidOutput)?;
    let response_json: serde_json::Value =
        serde_json::from_str(&response_text).map_err(|_| AttemptError::InvalidOutput)?;
    let content = response_json
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or(AttemptError::InvalidOutput)?;
    parse_candidates(
        mode,
        content,
        local_candidates,
        provider_source(config),
        config,
    )
}

fn provider_url(configured: &str) -> String {
    let base = configured.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

fn provider_source(config: &AppConfig) -> &'static str {
    match config.llm.provider.trim().to_ascii_lowercase().as_str() {
        "local" | "ollama" => "local-model",
        _ => "remote-provider",
    }
}

fn filtered_recent_commands(memory: &Memory<'_>, config: &AppConfig) -> Vec<String> {
    let mut commands = memory
        .commands
        .iter()
        .rev()
        .filter(|event| {
            !event.command.chars().any(char::is_control)
                && privacy::rejection_reason(&event.command, config).is_none()
        })
        .map(|event| event.command.trim().to_string())
        .filter(|command| !command.is_empty())
        .take(MAX_CONTEXT_COMMANDS)
        .collect::<Vec<_>>();
    commands.reverse();
    commands
}

fn parse_candidates(
    mode: Mode,
    content: &str,
    local_candidates: &[String],
    source: &'static str,
    config: &AppConfig,
) -> Result<Vec<ExternalCandidate>, AttemptError> {
    let parsed: serde_json::Value =
        serde_json::from_str(json_object(content)).map_err(|_| AttemptError::InvalidOutput)?;
    let commands = parsed
        .get("commands")
        .and_then(serde_json::Value::as_array)
        .ok_or(AttemptError::InvalidOutput)?;
    let allowed: HashSet<&str> = local_candidates.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let mut safe = Vec::new();
    for value in commands {
        let Some(command) = value.as_str().map(str::trim) else {
            continue;
        };
        if (mode == Mode::Rerank && !allowed.contains(command))
            || privacy::model_candidate_rejection_reason(command, config).is_some()
            || is_ignored_command(main_cmd(command), config)
            || !seen.insert(command.to_string())
        {
            continue;
        }
        safe.push(command.to_string());
        if safe.len() == MAX_CANDIDATES {
            break;
        }
    }
    if safe.is_empty() {
        return Err(AttemptError::InvalidOutput);
    }
    let candidate_count = safe.len();
    Ok(safe
        .into_iter()
        .enumerate()
        .map(|(rank, command)| ExternalCandidate {
            command,
            source,
            rank,
            candidate_count,
        })
        .collect())
}

fn json_object(content: &str) -> &str {
    let trimmed = content.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    unfenced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_urls_are_normalized_without_a_vendor_branch() {
        assert_eq!(
            provider_url("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
        assert_eq!(
            provider_url("https://provider.example/v1"),
            "https://provider.example/v1/chat/completions"
        );
    }

    #[test]
    fn rerank_accepts_only_safe_local_candidates() {
        let local = vec!["cargo test".to_string(), "cargo clippy".to_string()];
        let parsed = parse_candidates(
            Mode::Rerank,
            r#"{"commands":["cargo clippy","cargo publish"]}"#,
            &local,
            "local-model",
            &AppConfig::default(),
        )
        .expect("parse rerank");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].command, "cargo clippy");
    }

    #[test]
    fn generated_secrets_and_control_characters_are_untrusted() {
        let parsed = parse_candidates(
            Mode::Generate,
            r#"{"commands":["deploy --token model-secret-value","cargo test\nrm -rf target","cargo check"]}"#,
            &[],
            "remote-provider",
            &AppConfig::default(),
        )
        .expect("parse generated candidates");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].command, "cargo check");
    }

    #[test]
    fn fenced_json_is_still_parsed_as_untrusted_candidate_data() {
        let parsed = parse_candidates(
            Mode::Repair,
            "```json\n{\"commands\":[\"git status\"]}\n```",
            &[],
            "local-model",
            &AppConfig::default(),
        )
        .expect("parse fenced model response");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].command, "git status");
    }
}
