# Contextual prediction policy

The contextual policy is the local-only default in v0.5.0 after passing the replay promotion gate. Switch between it and the v0.4 fallback with:

```bash
soon config set prediction.policy contextual
soon config set prediction.policy v0.4-baseline
```

Run `soon replay` after installing v0.5.0; it evaluates the contextual default and `v0.4-baseline` on the same chronological prefixes without exposing command text.

## Boundaries

Candidate retrieval and ranking are separate interfaces in `src/prediction.rs`:

- `CandidateSource` retrieves safe, complete command strings and attaches a source to every candidate.
- `Ranker` selects one candidate from the retrieved set.

The v0.4 baseline retrieves matching first-order transitions and applies its deterministic frequency, result, directory, and recency ordering. The contextual policy retrieves complete commands from event history. When any first-order transition is available, unrelated candidates are removed before ranking; otherwise the full safe history provides a cold-start fallback.

Opt-in model candidates enter this same contextual ranker through the external candidate-source boundary. Their provider contract, model-order feature, deadline, filtering, and deterministic fallback are documented in [Optional model candidate sources](model-sources.md).

## Evidence

The contextual ranker combines these local signals:

| Signal | Historical comparison |
|---|---|
| First-order transition | command immediately before the candidate equals the completed command |
| Second-order transition | command before that transition equals the current event's predecessor |
| Directory | candidate event and current event have the same cwd |
| Repository and branch | optional values captured by reading `.git/HEAD`, without spawning `git` |
| Time | six four-hour UTC buckets plus weekday from the original event timestamp |
| Result | success/failure class of the historical predecessor and current event |
| Duration | predecessor duration in `<1s`, `1-10s`, `10-60s`, or `>=60s` |
| Frequency and recency | smoothed occurrence prior and normalized latest event position |
| Feedback | accepted adds 2 evidence units; executed adds 4; shown and dismissed add none |

Missing values contribute no term. Imported history therefore does not acquire an invented time, directory, result, duration, repository, or branch.

## Smoothing and ordering

For a categorical signal with `m` matches among `n` known observations and `k` possible buckets, the ranker adds the log likelihood ratio:

```text
ln(((m + 1) * k) / (n + k))
```

This is additive smoothing relative to a uniform prior. The frequency prior is `ln((occurrences + 1) / (all occurrences + candidate count))`.

The log-linear weights are:

| Signal | Weight |
|---|---:|
| Second-order transition | 2.50 |
| First-order transition | 2.00 |
| Directory | 1.50 |
| Repository | 1.25 |
| Branch | 1.25 |
| Result | 1.50 |
| Hour bucket | 0.75 |
| Weekday | 0.75 |
| Duration bucket | 0.75 |
| Feedback `ln(1 + units)` | 1.50 |
| Frequency prior | 1.00 |
| Normalized recency | 0.25 |

Scores are ranking evidence, not calibrated confidence. Ties resolve by evidence count, latest event index, then the complete command string, making output deterministic across processes.

## Debug and promotion gate

`soon --debug ... now --raw` leaves the raw command on stdout and prints only the selected policy, candidate source, and contributing signal-group names to stderr. It does not print candidates, stored context values, or a confidence claim.

`soon replay` reports coverage, exact top-1 match, p50, and p95 for both `v0.4-baseline` and `contextual-policy`. The contextual promotion gate passes only when contextual top-1 is strictly better, coverage is no worse, and contextual p95 is at most 20 ms. The gate is evidence for a release decision; running Replay does not edit configuration.
