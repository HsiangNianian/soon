<div align="center">

# soon

**The next command, before you type it.**

A local-first terminal agent that predicts, repairs, and suggests your next full command.

[![crates.io](https://img.shields.io/crates/v/soon.svg)](https://crates.io/crates/soon)
[![PyPI](https://img.shields.io/pypi/v/soon-bin.svg)](https://pypi.org/project/soon-bin/)
[![CI](https://github.com/HsiangNianian/soon/actions/workflows/proof-pr.yml/badge.svg)](https://github.com/HsiangNianian/soon/actions/workflows/proof-pr.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[Website](https://soon.hydroroll.team) · [v0.4.2 Zsh parser patch](https://github.com/HsiangNianian/soon/releases/tag/v0.4.2) · [Roadmap](https://github.com/users/HsiangNianian/projects/7)

</div>

> **Beta status:** v0.4.2 ships the opt-in Zsh ghost-suggestion loop, a privacy-safe adoption report, and correct handling for compound commands in Zsh history. Interactive integration is supported on native Linux and macOS; other shells and packaged platforms remain experimental unless listed in the [release contract](RELEASING.md).

<a href="https://soon.hydroroll.team">
  <img src="www/assets/soon-demo.svg" alt="An 18-second terminal demo: a failed Git command triggers a local Repair suggestion, Ctrl-F accepts it into the editable buffer, and the user decides when to execute it.">
</a>

## Try the beta

Install the same v0.4.2 release through Cargo or PyPI:

```bash
cargo install soon
# or: python -m pip install soon-bin
```

Enable the integration for the current Zsh session:

```zsh
eval "$(soon init zsh)"
```

At an empty prompt, soon computes in the background and renders one dim full-command suggestion. Press `Ctrl-F` to place it in the editable buffer, then review, edit, or run it yourself. Start typing to ignore it. soon never presses Enter for you.

The default prediction path uses local history, not a model or network service. It can choose a Next-step suggestion after success, a Repair suggestion after failure, or predict on demand when you run `soon`.

> **v0.5 development:** the source tree defaults to the local contextual policy after it passed the replay promotion gate. The published v0.4.2 package still uses the deterministic baseline.

## The idea

Your shell already knows what you typed. soon asks a different question: given the workflow you just followed, what complete command are you likely to run next?

The agent is a small local feedback loop:

1. Observe the completed command and its result.
2. Remember only safe, useful local context.
3. Rank complete commands from recurring transitions.
4. Render one editable ghost suggestion without blocking the prompt.
5. Learn whether it was accepted, executed, or dismissed.

## Why another shell tool?

| Tool category | You provide | It returns |
|---|---|---|
| History search, including Atuin and McFly | A query or key chord | A past command matching the search |
| Prefix autosuggestions in Fish or Zsh | The beginning of a command | A completion for that prefix |
| Prompt-driven command generation | A natural-language request | A newly generated command |
| **soon's target interaction** | **An empty prompt after a familiar workflow** | **One predicted full command, locally** |

soon is not trying to replace search or completion. The product succeeds only if accepting a correct prediction is faster than recalling and typing the command yourself.

## Keep it enabled

Add the same `eval` line to `~/.zshrc`. When no suggestion is visible, `Ctrl-F` keeps its previous behavior. To remove the hooks and restore the previous binding in the current session, run:

```zsh
soon-disable
```

The last background prediction latency is available without printing command content:

```zsh
print -r -- "$SOON_LAST_LATENCY_MS ms"
```

Cargo and PyPI are published together from one versioned tag. AUR and standalone binaries remain unsupported until they have tested artifact workflows. To install the current development branch instead:

```bash
cargo install --git https://github.com/HsiangNianian/soon
```

The supported interactive beta surface is Zsh on Linux and macOS. Other history parsers and packaged platforms remain on-demand or experimental. Maintainer guarantees are documented in [RELEASING.md](RELEASING.md).

## Use the current prototype

```bash
# Predict one full command from the detected shell history
soon

# Inspect the matching evidence
soon --debug

# Confirm which shell and history source were detected
soon which

# Show the most common executables in local history
soon stats

# Measure the current policy against past local transitions
soon replay

# Compare both policies, or switch back to the v0.4 fallback
soon config set prediction.policy v0.4-baseline

# Export aggregate adoption metrics that are safe to share
soon report
soon report --json

# Explicitly ask a configured OpenAI-compatible provider; never auto-executes
soon generate
```

Override shell detection when needed:

```bash
soon --shell zsh
```

The current source can parse Bash, Zsh, Fish, Nushell, Elvish, PowerShell, and tcsh history. Parsing a format does not mean interactive integration for that shell is complete.

## What v0.4 ships

| Shipped surface | Release evidence |
|---|---|
| On-demand command plus opt-in Zsh loop | Clean-session install-to-uninstall smoke test |
| Manual, successful-command Next-step, and failed-command Repair triggers | Native Linux and macOS lifecycle regression coverage |
| Retained events plus chronological quality and latency replay | Deterministic fixture with a 20 ms Zsh p95 budget |
| Sensitive-command filters plus idempotent Zsh history import | Documented privacy behavior and aggregate-only inspection |

The implementation plan lives in [RFC #4](https://github.com/HsiangNianian/soon/issues/4). Work is tracked in the public [Personal Terminal Agent Project](https://github.com/users/HsiangNianian/projects/7).

## How the current predictors work

The v0.4 baseline matches recurring transitions, preserves complete historical commands including arguments, and deterministically ranks result, directory, frequency, and recency evidence.

The contextual policy separates candidate retrieval from ranking, then combines first- and second-order transitions, cwd, optional repository and branch, original time, result, duration, frequency, recency, and accepted/executed feedback. Missing metadata contributes no evidence. Its additive smoothing, weights, debug contract, and deterministic tie-breaking are documented in [Contextual prediction policy](docs/contextual-policy.md).

Optional local and OpenAI-compatible sources can rerank safe history candidates, suggest a Repair after failure, or Generate only on explicit request. They have a hard deadline and silently retain the deterministic result on timeout, unavailability, or invalid output. The default is `prediction.model_mode = "off"`. The exact provider payload, output filters, ranking feature, and fallback protocol are documented in [Optional model candidate sources](docs/model-sources.md).

The [small local model gate](docs/model-evaluation.md) tested Qwen2.5-Coder 0.5B Q4_K_M on an Intel Mac. It added one exact cold-start Repair across eight public fixtures at 676.2 ms p95, which was enough to validate the optional path but not enough quality evidence to enable it by default.

The v0.5 source promotes contextual after it improved exact top-1 without reducing coverage or exceeding the 20 ms p95 budget in both the deterministic fixture and a local aggregate replay. `v0.4-baseline` remains an explicit offline fallback.

## Agent roadmap

The [v0.4 Local Agent MVP](https://github.com/HsiangNianian/soon/milestone/1) combines safe command lifecycle events, explicit `soon`, Next-step after success, Repair after failure, a private history-import path, and measured local replay.

The closed [v0.4.1 Adoption Sprint](https://github.com/HsiangNianian/soon/milestone/3) added the public demo, launch assets, and privacy-safe report. The planned ten-user pilot and final timed launch measurements were explicitly closed as not planned rather than reported as completed.

The [v0.5 Hybrid Prediction Engine](https://github.com/HsiangNianian/soon/milestone/2) promotes the contextual probabilistic ranker from [#16](https://github.com/HsiangNianian/soon/issues/16) and adds the opt-in local-model/OpenAI-compatible layer from [#17](https://github.com/HsiangNianian/soon/issues/17). Model output is never required for the default hot path.

## Privacy

With the default `prediction.model_mode = "off"`, `soon`, `soon now`, `soon replay`, `soon report`, `soon init zsh`, and the local learning commands read files on your machine and do not require a network service. The Zsh integration invokes the predictor in a background process and does not block the prompt while waiting for a result. An opt-in model mode or explicit `soon generate` may contact only the configured provider.

The current source rejects likely inline API keys, tokens, authorization headers, password flags, private-key material, and known credential prefixes before storing a command or suggestion. It applies the same filter again before ranking or rendering shell history, old event data, legacy learn data, provider context, and model output. Rejections report a category, not the command text.

Add exact case-sensitive exclusions or regular-expression exclusions without editing stored data:

```bash
soon config set privacy.excluded_literals 'company-deploy --production'
soon config set privacy.excluded_patterns '(?i)^kubectl .*--context production'
```

Comma-separated values configure more than one exclusion. Literal values are redacted from `soon config`, `config get`, and successful `config set` output. Invalid regular expressions are rejected before the configuration is saved.

The v0.5 model candidate path sends at most six filtered commands, the current command and result class, and at most five local candidates to an OpenAI-compatible endpoint. It omits paths, repository metadata, event IDs, timestamps, duration, feedback, stdout, and stderr. Model output is filtered again for control characters, credentials, configured exclusions, and dangerous commands before entering the contextual ranker. The older `soon learn ask` experiment remains separate and includes the filtered current directory.

Provider credentials are read at request time from an environment variable and are never stored or printed by soon. The default variable is `SOON_LLM_API_KEY`; configure a different variable name with `llm.api_key_env`. For example, this Zsh flow keeps the value out of shell history:

```zsh
soon config set llm.provider openai-compatible
soon config set llm.api_url https://provider.example/v1
read -rs 'SOON_LLM_API_KEY?API key: '
print
export SOON_LLM_API_KEY
```

A local OpenAI-compatible endpoint can run without a credential. The legacy `llm.api_key` setting is rejected.

The Zsh lifecycle integration stores local command and suggestion events in a retained JSONL log under the operating system's application-data directory. When cwd is inside a Git worktree, the background recorder also reads the repository root and symbolic branch directly from `.git/HEAD`; it does not spawn `git`. Inspect the store's exact path, schema version, retention, and aggregate counts without printing command text:

```bash
soon events inspect
```

The default retention is 10,000 events. It is user-controlled, and clearing requires explicit confirmation:

```bash
soon config set events.retention 5000
soon events clear --yes
```

Give a fresh profile useful event memory by previewing a Zsh history import first:

```bash
# Uses ~/.zsh_history
soon events import-zsh --preview
soon events import-zsh

# Or pass current and rotated files explicitly, oldest first
soon events import-zsh --preview \
  --path ~/.zsh_history.1 \
  --path ~/.zsh_history
soon events import-zsh \
  --path ~/.zsh_history.1 \
  --path ~/.zsh_history
```

Plain command-per-line history and Zsh extended history (`: <epoch>:<duration>;<command>`) are supported. Extended timestamps and durations are preserved; unavailable cwd, exit status, and plain-history timestamps remain unknown. Preview and import summaries report importable, sensitive, malformed, duplicate, and already-imported counts without printing command text. Stable event IDs make repeated imports idempotent, including identical rotated files.

Compare the v0.4 baseline and contextual policy against that local event memory:

```bash
soon replay
```

Replay follows JSONL append order rather than event timestamps. For each linked command transition it predicts first, scores the result, and only then exposes that transition to later samples, so future observations cannot leak into training. Unknown exit status is classified as `manual`; exit status zero is `next-step`; any other status is `repair`.

`Samples` counts eligible linked transitions. Coverage is predictions divided by samples, and top-1 match is exact command matches divided by all samples. Overall and per-trigger rows preserve the v0.4 baseline metrics. The policy comparison runs both baseline and contextual prediction through the same production module and reports coverage, top-1, p50, and p95 for each. Candidate-source rows retain the computed deterministic-history baseline and separately score other suggestions that were actually recorded before the next command. Model attempts aligned to a later command also report timeout, invalid-output, and deterministic-fallback rates.

The report is aggregate-only: it prints no command text and performs no upload. The deterministic CI fixture has a Zsh hot-path p95 budget of **20 ms**. Replay also prints a contextual promotion gate that requires strictly better top-1, no coverage regression, and contextual p95 at or below that budget.

Export the smaller adoption report when sharing beta feedback:

```bash
soon report
soon report --json
```

This is an explicit, offline read of the same local event store. Both forms contain aggregate counters and latency distributions only—never raw commands, arguments, paths, hostnames, usernames, event IDs, timestamps, or database rows. The human form is designed for an issue or discussion; `--json` is suitable for scripts and uses schema version `2`:

| JSON field | Meaning |
|---|---|
| `schema_version` | Report schema version; currently `2` |
| `samples.eligible_transitions` | Linked command transitions eligible for chronological replay |
| `samples.predictions` | Eligible transitions for which the deterministic replay produced a prediction |
| `samples.prediction_coverage_percent` | `predictions / eligible_transitions * 100` |
| `suggestions.shown` | Retained suggestion events with the `shown` outcome |
| `suggestions.accepted` | Retained suggestion events with the `accepted` outcome |
| `suggestions.acceptance_percent` | `accepted / shown * 100` |
| `suggestions.executed` | Retained suggestion events with the `executed` outcome |
| `suggestions.execution_percent` | `executed / shown * 100` |
| `latency_ms.replay.samples` | Eligible transitions benchmarked by chronological replay |
| `latency_ms.replay.p50`, `latency_ms.replay.p95` | Offline replay-computation percentiles in milliseconds |
| `latency_ms.suggestion.samples` | Valid shell-observed latency samples from `shown` events |
| `latency_ms.suggestion.p50`, `latency_ms.suggestion.p95` | Shell-observed suggestion-result percentiles in milliseconds |

A percentage is `null` when its denominator is zero. Each latency distribution has an explicit sample count and returns `null` percentiles when that count is zero. Suggestion latency counts only valid non-negative `shown` rows, so later accepted, executed, or dismissed outcomes do not duplicate one displayed suggestion. Human output prints `n/a` for unavailable distributions.

Schema version 2 replaces the version 1 `latency_ms.p50` and `latency_ms.p95` fields with `latency_ms.replay` and `latency_ms.suggestion`. Use the suggestion distribution for user-facing adoption studies and the replay distribution for policy benchmarking.

Config lives at `~/.config/soon/config.toml`:

```bash
soon config init
soon config path
soon config get general.ngram
soon config set general.ngram 5
soon config get prediction.policy
soon config set prediction.policy v0.4-baseline  # explicit fallback
soon config get prediction.model_mode            # off by default
soon config set prediction.model_mode repair      # opt-in model path
soon config set prediction.model_timeout_ms 1500
soon config set update.channel cargo  # or pip
```

## Commands

```text
soon                    Predict the next full command
soon now                Run the same prediction explicitly
soon generate           Explicitly request a model candidate
soon init zsh           Print the opt-in Zsh integration
soon stats              Show the most-used executables
soon which              Show shell and history diagnostics
soon config             View or change local configuration
soon events             Inspect, clear, or import local agent events
soon replay             Measure local prediction quality and latency
soon report             Export privacy-safe aggregate adoption metrics
soon learn              Use the experimental learning tools
soon update             Check the configured release channel
```

## Contributing

Start with [RFC #4](https://github.com/HsiangNianian/soon/issues/4), then choose an open issue from the [v0.5 Hybrid Prediction Engine](https://github.com/HsiangNianian/soon/milestone/2). The contextual ranker is tracked in [#16](https://github.com/HsiangNianian/soon/issues/16), and the opt-in model candidate layer is tracked in [#17](https://github.com/HsiangNianian/soon/issues/17).

For local verification:

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- --help
```

## License

[MIT](LICENSE) © 2025-present [HsiangNianian](https://github.com/HsiangNianian)
