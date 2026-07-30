# Optional model candidate sources

Model-backed candidates are opt-in. `prediction.model_mode` defaults to `off`, so `soon`, `soon now`, and the Zsh hooks do not contact a provider unless the user changes that setting. `soon generate` is the separate explicit generation path.

All new model paths use an OpenAI-compatible Chat Completions endpoint. The policy layer does not branch on a model vendor. `llm.provider` only controls source attribution: `local` and the legacy `ollama` name record `local-model`; other values record `remote-provider`.

## Configuration

Run a small model through a local OpenAI-compatible endpoint:

```bash
soon config set llm.provider local
soon config set llm.api_url http://127.0.0.1:11434/v1
soon config set llm.model qwen2.5-coder:1.5b
soon config set prediction.model_timeout_ms 1500
soon config set prediction.model_mode repair
```

Use any remote OpenAI-compatible service for explicit generation:

```bash
soon config set llm.provider openai-compatible
soon config set llm.api_url https://provider.example/v1
soon config set llm.model provider-model-name
soon config set llm.api_key_env SOON_LLM_API_KEY
soon generate
```

The credential value is read from the named environment variable at request time. It is not saved in `config.toml` or printed by soon.

Automatic modes are:

| `prediction.model_mode` | Behavior |
|---|---|
| `off` | Default. No model request on the ordinary prediction path. |
| `rerank` | Ask the model to order up to five safe local candidates after any completed command. |
| `repair` | Ask for a minimal correction only after a non-zero exit status. |
| `rerank-repair` | Rerank after success and request a Repair candidate after failure. |

`soon generate` always means an explicit Generate attempt; it does not enable an automatic mode. Every output is only printed or placed in the editable Zsh buffer. No model path executes a candidate or presses Enter.

## Provider contract

The provider receives one system message and one user message. Depending on the mode, the user message contains only:

- mode: `rerank`, `repair`, or `generate`;
- the current command when it passes the privacy filter;
- the previous result class: `success`, `failure`, or unknown;
- at most six recent safe commands for Rerank or Generate;
- at most five safe local candidates. Repair intentionally omits recent history and sends only the filtered failed command plus this shortlist.

It does not include cwd, repository paths, branch names, event IDs, timestamps, duration, feedback, stdout, or stderr. Events rejected by built-in or configured privacy rules are removed before the payload is built.

The response message content must be strict JSON:

```json
{"commands":["cargo test --workspace","cargo clippy --all-targets"]}
```

Rerank output may only contain commands from the supplied local shortlist. Repair and Generate may propose a command absent from history. In every mode, output is treated as untrusted: empty strings, control characters, credentials, configured exclusions, ignored executables, forced recursive deletion, device writes, destructive Git reset/clean/push, remote-script pipes, and system shutdown commands are rejected before ranking or rendering. Responses are capped at 64 KiB.

## Deadline, ranking, and fallback

`prediction.model_timeout_ms` is a hard whole-request deadline from 10 to 30,000 ms; the default is 1,500 ms. Timeout, unavailable provider, malformed JSON, and a response with no safe candidate silently retain the deterministic result.

Safe model candidates enter the same contextual ranker as historical candidates. Model order contributes `8 * ln(1 + remaining_rank)` as an uncalibrated ranking feature; transition, context, feedback, frequency, recency, and deterministic tie-breaking still apply. A selected model candidate is attributed to `local-model` or `remote-provider`. A failed attempt that returns the local result is attributed to `deterministic-fallback` and records `timeout`, `invalid-output`, or `deterministic-fallback` as its model outcome.

The Zsh integration records that source and outcome on shown, accepted, executed, and dismissed events. `soon replay` reports source-specific coverage, top-1, p50, and p95, plus aggregate model timeout, invalid-output, and unavailable-provider fallback rates. It never prints command text.

## Evaluation gate

The deterministic contextual policy stays the default. A small local model remains optional unless chronological Repair or cold-start top-1 improves and its observed latency stays within the configured model deadline. Mock-provider tests cover the protocol without credentials. The reproducible [0.5B local-model evaluation](model-evaluation.md) records the real model, quantization, machine, fixture, quality, latency, resource use, and the decision to keep it opt-in.
