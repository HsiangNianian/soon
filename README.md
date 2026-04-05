<div align="center">

# soon

**Predict your next shell command before you type it.**

[![crates.io](https://img.shields.io/crates/v/soon.svg)](https://crates.io/crates/soon)
[![PyPI](https://img.shields.io/pypi/v/soon-bin.svg)](https://pypi.org/project/soon-bin/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Publish to crates.io](https://github.com/HsiangNianian/soon/actions/workflows/publish-crates.yml/badge.svg)](https://github.com/HsiangNianian/soon/actions/workflows/publish-crates.yml)
[![Publish to PyPI](https://github.com/HsiangNianian/soon/actions/workflows/publish-pypi.yml/badge.svg)](https://github.com/HsiangNianian/soon/actions/workflows/publish-pypi.yml)

A command-line agent that learns your shell habits — tracking command sequences, time patterns, and directory context — to predict what you'll type next.

Not autocomplete. **Precognition.**

<img src="image/showcase.jpg" width="600" alt="Soon showcase" />

</div>

---

## Features

| | Feature | Description |
|---|---|---|
| **7 shells** | bash · zsh · fish · nushell · elvish · PowerShell · tcsh |
| **Learn** | Ingests history across all shells, builds transition graphs, time/dir patterns, and trigram vectors |
| **Markov** | Blended order-1 / order-2 Markov chain for sequence prediction |
| **Fuzzy** | Character-trigram TF-IDF similarity search across your command vocabulary |
| **LLM** | Optional OpenAI / Ollama integration — send context, get JSON predictions |
| **Config** | TOML config at `~/.config/soon/config.toml`, CLI-editable |
| **Update** | Self-update via `soon update` — auto-detects cargo / pip / AUR |

## Install

```bash
# Cargo (recommended)
cargo install soon

# Python
pip install soon-bin

# Arch Linux (AUR)
paru -Sy soon
```

## Quick Start

```bash
# Predict your next command
soon

# Learn from your current shell history
soon learn ingest

# Learn from ALL shells at once
soon learn ingest-all

# See intelligent predictions
soon learn predict

# Initialize config file
soon config init
```

## Commands

```
soon                    Predict next command (default)
soon now                Same as above, explicit
soon stats              Show your most used commands
soon which              Show detected shell & diagnostics
soon update             Self-update to latest version
soon config             View / manage configuration
soon learn              Intelligent learning & prediction
```

### `soon learn`

```
soon learn              Show status & available actions
soon learn ingest       Ingest current shell history
soon learn ingest-all   Ingest from all detected shells
soon learn stats        Show learn database statistics
soon learn predict      Predict using learned patterns
soon learn similar <q>  Fuzzy-find similar commands
soon learn ask          Ask LLM for predictions
soon learn reset        Reset the learn database
```

### `soon config`

```
soon config             Show all configuration
soon config init        Create default config file
soon config path        Print config file path
soon config get <KEY>   Get a value  (e.g. general.ngram)
soon config set <K> <V> Set a value  (e.g. general.ngram 5)
```

<details>
<summary><b>Configuration reference</b></summary>

```toml
# ~/.config/soon/config.toml

[general]
shell = "auto"                    # auto / bash / zsh / fish / nushell / elvish / powershell / tcsh
ngram = 3                         # n-gram window for classic prediction
ignored_commands = ["soon", "cd", "ls", "pwd", "exit", "clear"]

[update]
channel = "auto"                  # auto / cargo / pip / aur / binary

[llm]
provider = ""                     # openai / ollama
api_url = ""                      # e.g. https://api.openai.com or http://localhost:11434
api_key = ""                      # your API key (leave empty for Ollama)
model = ""                        # e.g. gpt-4o-mini, llama3.2
prompt = ""                       # custom prompt (use {commands} and {directory} placeholders)
```

</details>

## How It Works

### Classic Prediction (`soon now`)

Reads the last N commands from your shell history, scans for matching n-gram patterns, and scores candidates by recency and match ratio.

### Learned Prediction (`soon learn predict`)

Uses a multi-signal fusion engine:

| Signal | Weight | Description |
|--------|--------|-------------|
| Bigram transition | 35% | What usually follows the last 2 commands |
| Single transition | 25% | What usually follows the last command |
| Directory context | 20% | What you run in this directory |
| Time-of-day | 15% | What you run at this hour |
| Day-of-week | 5% | Weekday vs weekend patterns |

Fallback: Blended Markov chain (order-1 + order-2).

### Fuzzy Search (`soon learn similar`)

Character-trigram TF-IDF vectors with cosine similarity — finds commands that *look like* your query even with typos.

### LLM Mode (`soon learn ask`)

Sends your recent commands + current directory + time to a configured OpenAI-compatible or Ollama endpoint. The LLM returns JSON predictions with confidence scores and reasoning.

```bash
# OpenAI setup
soon config set llm.provider openai
soon config set llm.api_url https://api.openai.com
soon config set llm.api_key sk-...

# Ollama setup (local, no key needed)
soon config set llm.provider ollama
soon config set llm.api_url http://localhost:11434
```

## Options

| Flag | Description |
|------|-------------|
| `--shell <SHELL>` | Override shell detection |
| `--ngram <N>` | Set n-gram window size |
| `--debug` | Show debug output |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

---

<div align="center">

MIT &copy; 2025-PRESENT [HsiangNianian](https://github.com/HsiangNianian)

[![FOSSA Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2FHsiangNianian%2Fsoon.svg?type=shield&issueType=security)](https://app.fossa.com/projects/git%2Bgithub.com%2FHsiangNianian%2Fsoon?ref=badge_shield&issueType=security)

</div>
