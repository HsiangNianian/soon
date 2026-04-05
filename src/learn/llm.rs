use crate::config::AppConfig;

/// LLM-enhanced prediction.
/// Sends context (recent commands, current dir, time) to a configured LLM
/// and parses the JSON response for command predictions.
pub struct LlmPrediction {
    pub command: String,
    pub confidence: f64,
    pub reason: String,
}

/// Check if LLM is configured and available.
pub fn is_configured(config: &AppConfig) -> bool {
    !config.llm.provider.is_empty() && !config.llm.api_url.is_empty()
}

/// Build the prompt for the LLM.
fn build_prompt(
    recent_cmds: &[&str],
    current_dir: Option<&str>,
    custom_prompt: &str,
) -> String {
    let cmds_str = recent_cmds.join("\n");
    let dir_str = current_dir.unwrap_or("unknown");

    if !custom_prompt.is_empty() {
        return custom_prompt
            .replace("{commands}", &cmds_str)
            .replace("{directory}", dir_str);
    }

    format!(
        r#"You are a shell command prediction assistant. Based on the user's recent command history and current working directory, predict the most likely next commands they will run.

Recent commands (most recent last):
{cmds_str}

Current directory: {dir_str}

Respond ONLY with valid JSON in this exact format, no other text:
{{"predictions":[{{"command":"<cmd>","confidence":<0.0-1.0>,"reason":"<brief reason>"}}]}}

Return up to 3 predictions, sorted by confidence (highest first)."#
    )
}

/// Call the LLM API and return predictions.
pub fn predict(
    config: &AppConfig,
    recent_cmds: &[&str],
    current_dir: Option<&str>,
    _n: usize,
) -> Result<Vec<LlmPrediction>, String> {
    if !is_configured(config) {
        return Err("LLM not configured. Use `soon config set llm.provider <provider>` and `soon config set llm.api_url <url>`.".to_string());
    }

    let prompt = build_prompt(recent_cmds, current_dir, &config.llm.prompt);

    let api_url = config.llm.api_url.trim_end_matches('/');
    let url = match config.llm.provider.as_str() {
        "openai" => format!("{}/v1/chat/completions", api_url),
        "ollama" => format!("{}/api/chat", api_url),
        _ => format!("{}/v1/chat/completions", api_url), // default to OpenAI-compatible
    };

    let model = if config.llm.model.is_empty() {
        match config.llm.provider.as_str() {
            "ollama" => "llama3.2",
            _ => "gpt-4o-mini",
        }
    } else {
        &config.llm.model
    };

    let request_body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "You are a shell command prediction assistant. Respond only with valid JSON."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 256
    });

    let mut req = ureq::post(&url).header("Content-Type", "application/json");

    if !config.llm.api_key.is_empty() {
        req = req.header(
            "Authorization",
            &format!("Bearer {}", config.llm.api_key),
        );
    }

    let mut response = req
        .send(serde_json::to_string(&request_body).unwrap().as_bytes())
        .map_err(|e| format!("LLM API request failed: {}", e))?;

    let body_str = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read LLM response: {}", e))?;

    let body: serde_json::Value =
        serde_json::from_str(&body_str).map_err(|e| format!("Invalid JSON response: {}", e))?;

    // Extract the content from the response
    let content = extract_content(&body, &config.llm.provider)?;

    // Parse predictions from the content
    parse_predictions(&content)
}

/// Extract the message content from different API response formats.
fn extract_content(body: &serde_json::Value, provider: &str) -> Result<String, String> {
    match provider {
        "ollama" => body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Failed to extract content from Ollama response".to_string()),
        _ => {
            // OpenAI-compatible format
            body.get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "Failed to extract content from API response".to_string())
        }
    }
}

/// Parse predictions from the LLM's JSON output.
fn parse_predictions(content: &str) -> Result<Vec<LlmPrediction>, String> {
    // Try to find JSON in the content (LLM might wrap it in markdown etc.)
    let json_str = extract_json_block(content);

    let parsed: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse LLM JSON: {}", e))?;

    let predictions = parsed
        .get("predictions")
        .and_then(|p| p.as_array())
        .ok_or_else(|| "Missing 'predictions' array in LLM response".to_string())?;

    let mut results = Vec::new();
    for pred in predictions {
        let command = pred
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let confidence = pred
            .get("confidence")
            .and_then(|c| c.as_f64())
            .unwrap_or(0.0);
        let reason = pred
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();

        if !command.is_empty() {
            results.push(LlmPrediction {
                command,
                confidence,
                reason,
            });
        }
    }

    Ok(results)
}

/// Extract a JSON block from potentially messy LLM output.
fn extract_json_block(s: &str) -> &str {
    // Try to find ```json ... ``` block
    if let Some(start) = s.find("```json") {
        let start = start + 7;
        if let Some(end) = s[start..].find("```") {
            return s[start..start + end].trim();
        }
    }
    // Try to find ``` ... ``` block
    if let Some(start) = s.find("```") {
        let start = start + 3;
        if let Some(end) = s[start..].find("```") {
            return s[start..start + end].trim();
        }
    }
    // Try to find { ... } directly
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            return &s[start..=end];
        }
    }
    s.trim()
}
