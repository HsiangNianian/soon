#!/usr/bin/env python3
"""Run an aggregate-only local-model Repair evaluation against soon."""

from __future__ import annotations

import argparse
import math
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


CASES = (
    ("git statsu", "git status"),
    ("cargo tset", "cargo test"),
    ("npm tset", "npm test"),
    ("pyest tests/test_api.py", "pytest tests/test_api.py"),
    ("docker ps --al", "docker ps --all"),
    ("kubectl get pdoes", "kubectl get pods"),
    ("rg --fiels", "rg --files"),
    ("git chekcout main", "git checkout main"),
)


def run(soon: Path, home: Path, *args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.pop("SOON_LLM_API_KEY", None)
    env.update(
        {
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_DATA_HOME": str(home / ".local/share"),
        }
    )
    return subprocess.run(
        [str(soon), *args],
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def checked(soon: Path, home: Path, *args: str) -> None:
    result = run(soon, home, *args)
    if result.returncode != 0:
        raise RuntimeError(f"soon command failed while preparing evaluation: {args[0]}")


def predict(soon: Path, home: Path, failed: str) -> tuple[str, str, str, float]:
    started = time.perf_counter()
    result = run(
        soon,
        home,
        "now",
        "--raw",
        "--include-source",
        "--after",
        failed,
        "--exit-code",
        "1",
        "--event-id",
        "failed-current",
        "--cwd",
        "/tmp/soon-public-model-fixture",
    )
    latency_ms = (time.perf_counter() - started) * 1000
    if result.returncode != 0 and not result.stdout.strip():
        return "", "", "", latency_ms
    if result.returncode != 0:
        raise RuntimeError("soon prediction failed during evaluation")
    fields = result.stdout.rstrip("\n").split("\t", 2)
    if fields == [""]:
        return "", "", "", latency_ms
    if len(fields) == 2:
        return fields[0], "", fields[1], latency_ms
    if len(fields) == 3:
        return fields[0], fields[1], fields[2], latency_ms
    return "", "invalid-output", "", latency_ms


def percentile(values: list[float], quantile: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    rank = max(1, math.ceil(quantile * len(ordered)))
    return ordered[min(rank - 1, len(ordered) - 1)]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--soon", type=Path, default=Path("target/debug/soon"))
    parser.add_argument("--api-url", default="http://127.0.0.1:18089/v1")
    parser.add_argument(
        "--model", default="qwen2.5-coder-0.5b-instruct-q4_k_m.gguf"
    )
    parser.add_argument("--timeout-ms", type=int, default=10_000)
    args = parser.parse_args()

    soon = args.soon.resolve()
    if not soon.is_file():
        raise SystemExit("soon binary not found; run cargo build --locked first")

    baseline_predictions = 0
    baseline_matches = 0
    baseline_latencies: list[float] = []
    model_predictions = 0
    model_matches = 0
    model_latencies: list[float] = []
    outcomes: dict[str, int] = {}

    root = Path(tempfile.mkdtemp(prefix="soon-public-model-eval-"))
    try:
        for index, (failed, expected) in enumerate(CASES):
            home = root / str(index)
            home.mkdir(parents=True)
            checked(
                soon,
                home,
                "events",
                "record-command",
                "--id",
                "failed-current",
                "--command",
                failed,
                "--cwd",
                "/tmp/soon-public-model-fixture",
                "--started-at-ms",
                "1000",
                "--duration-ms",
                "25",
                "--exit-code",
                "1",
                "--shell",
                "zsh",
            )

            _, _, baseline, latency = predict(soon, home, failed)
            baseline_latencies.append(latency)
            baseline_predictions += int(bool(baseline))
            baseline_matches += int(baseline == expected)

            checked(soon, home, "config", "set", "llm.provider", "local")
            checked(soon, home, "config", "set", "llm.api_url", args.api_url)
            checked(soon, home, "config", "set", "llm.model", args.model)
            checked(
                soon,
                home,
                "config",
                "set",
                "prediction.model_timeout_ms",
                str(args.timeout_ms),
            )
            checked(soon, home, "config", "set", "prediction.model_mode", "repair")

            _, outcome, model, latency = predict(soon, home, failed)
            model_latencies.append(latency)
            model_predictions += int(bool(model))
            model_matches += int(model == expected)
            aggregate_outcome = outcome or "no-safe-candidate"
            outcomes[aggregate_outcome] = outcomes.get(aggregate_outcome, 0) + 1
    finally:
        shutil.rmtree(root)

    samples = len(CASES)
    print("Aggregate local-model Repair evaluation")
    print(f"Samples: {samples}")
    print(
        "contextual-policy: "
        f"coverage={baseline_predictions / samples * 100:.1f}% "
        f"top1={baseline_matches / samples * 100:.1f}% "
        f"p50={percentile(baseline_latencies, 0.50):.1f}ms "
        f"p95={percentile(baseline_latencies, 0.95):.1f}ms"
    )
    print(
        "local-model: "
        f"coverage={model_predictions / samples * 100:.1f}% "
        f"top1={model_matches / samples * 100:.1f}% "
        f"p50={percentile(model_latencies, 0.50):.1f}ms "
        f"p95={percentile(model_latencies, 0.95):.1f}ms"
    )
    print(
        "outcomes: "
        + ", ".join(f"{name}={count}" for name, count in sorted(outcomes.items()))
    )
    print("Privacy: public fixtures only; no user history or command text printed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
