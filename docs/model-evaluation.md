# Small local model evaluation

This is the release gate for the optional model candidate source in #17. The fixture and output are aggregate-only and reproducible; it does not read the maintainer's shell history.

## Environment

- Date: 2026-07-30
- Machine: macOS x86_64, Intel Core i9-9980HK, 32 GiB RAM
- Runner: [llama.cpp b10195](https://github.com/ggml-org/llama.cpp/releases/tag/b10195), official macOS x64 binary, version `10195 (47f686f53)`
- Model: [Qwen2.5-Coder-0.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF), `Q4_K_M`, model revision `ebb2015119c907b064c512bf053e945850b5875f`
- Model SHA-256: `1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32`
- Model file: 469 MiB; observed server RSS after evaluation: approximately 662 MiB
- Model deadline for the run: 10,000 ms. The release default remains 1,500 ms.

## Fixture

[`scripts/evaluate_model.py`](../scripts/evaluate_model.py) contains eight public failed-command typo fixtures. Each sample gets an isolated temporary HOME and event store. The contextual baseline and the local-model Repair path see the same failed command; no personal history, credentials, paths, stdout, or stderr enter the run.

Run against an already-started OpenAI-compatible local server:

```bash
cargo build --locked
python3 scripts/evaluate_model.py \
  --soon target/debug/soon \
  --api-url http://127.0.0.1:18089/v1 \
  --model qwen2.5-coder-0.5b-instruct-q4_k_m.gguf \
  --timeout-ms 10000
```

## Result

```text
Samples: 8
contextual-policy: coverage=0.0% top1=0.0% p50=16.7ms p95=24.7ms
local-model: coverage=12.5% top1=12.5% p50=408.2ms p95=676.2ms
outcomes: no-safe-candidate=7, success=1
```

An immediate warm repeat kept the same 1/8 quality result and measured contextual p50/p95 at 16.3/17.6 ms and local-model p50/p95 at 401.8/428.8 ms. The decision below uses the more conservative first-run model p95 rather than selecting the warmer number.

The fixture is intentionally cold-start Repair: it contains the failed command but no prior correction, so the history-only policy has no candidate. The 0.5B model produced one exact correction. Seven responses repeated the failed command; the contextual ranker rejected that self-candidate and rendered nothing.

## Decision

The model path stays optional. It demonstrated measurable cold-start coverage and stayed within the default 1,500 ms model deadline on this machine, but one exact match out of eight is not enough quality evidence to put inference on the default hot path. The deterministic contextual policy remains the default, and provider failures or unsafe/invalid responses continue to fall back locally when a deterministic candidate exists.
