# Changelog

All notable changes to soon are documented here.

## Unreleased

### Added

- Added a contextual full-command ranker with separate candidate-source
  and ranker boundaries, smoothed transition and context evidence, repository
  and branch capture, feedback-aware ranking, explainable debug output, and a
  chronological promotion comparison against the v0.4 baseline. It is the v0.5
  source default after passing the quality and latency gate; `v0.4-baseline`
  remains an explicit fallback ([#16](https://github.com/HsiangNianian/soon/issues/16)).
- Added opt-in local and OpenAI-compatible candidate sources for bounded
  rerank, Repair, and explicit Generate modes. Provider context is minimal and
  filtered, model output is treated as untrusted, deadlines fall back to the
  deterministic policy, and Zsh/replay retain source and outcome attribution
  ([#17](https://github.com/HsiangNianian/soon/issues/17)).

## 0.4.2 - 2026-07-29

soon 0.4.2 is a focused Zsh history-correctness patch. It keeps compound
commands intact across manual prediction and event import while preserving the
existing extended-history metadata behavior.

### Fixed

- Preserve plain Zsh history commands containing semicolons by sharing one
  decoder between on-demand prediction and event import
  ([#36](https://github.com/HsiangNianian/soon/pull/36)).

### Documentation

- Added the reusable v0.4.1 launch kit and its rendered campaign assets
  ([#33](https://github.com/HsiangNianian/soon/pull/33)).

[Full diff](https://github.com/HsiangNianian/soon/compare/v0.4.1...v0.4.2)

## 0.4.1 - 2026-07-27

soon 0.4.1 is the adoption build for the local terminal-agent beta. It makes
the product loop easier to evaluate and exposes privacy-safe measurements that
distinguish offline replay performance from the latency users experience in
the shell.

### Added

- Added a versioned landing page and deterministic terminal demo for the
  Repair, Ctrl-F acceptance, and explicit execution loop
  ([#28](https://github.com/HsiangNianian/soon/pull/28)).
- Added explicit `soon report` and `soon report --json` commands for sharing
  aggregate coverage, adoption, and latency metrics without command data
  ([#29](https://github.com/HsiangNianian/soon/pull/29)).

### Fixed

- Split chronological replay latency from shell-observed suggestion latency,
  and count each valid `shown` suggestion once instead of duplicating its
  accepted, executed, or dismissed lifecycle rows
  ([#30](https://github.com/HsiangNianian/soon/issues/30)).

### Changed

- Advanced the adoption report to schema version 2. Latency now contains
  separate `replay` and `suggestion` distributions with explicit sample
  counts and nullable p50/p95 values.
- Clarified that a research issue may remain open when it explicitly depends
  on the release artifact, avoiding a circular release gate.

[Full diff](https://github.com/HsiangNianian/soon/compare/v0.4.0...v0.4.1)

## 0.4.0 - 2026-07-27

soon 0.4.0 is the first local terminal-agent beta. Interactive integration is
supported for Zsh on native Linux and macOS. Suggestions are always editable
and are never executed automatically. Local and OpenAI-compatible model
providers remain optional; the default prediction path is deterministic and
local-first.

### Added

- Added an opt-in Zsh ghost-suggestion loop with manual, successful-command
  Next-step, and failed-command Repair triggers
  ([#15](https://github.com/HsiangNianian/soon/pull/15),
  [#19](https://github.com/HsiangNianian/soon/pull/19)).
- Added retained command and suggestion events, sensitive-command filtering,
  and previewable, idempotent Zsh history import
  ([#20](https://github.com/HsiangNianian/soon/pull/20),
  [#21](https://github.com/HsiangNianian/soon/pull/21)).
- Added chronological replay metrics for coverage, exact top-1 matches,
  trigger-specific results, candidate sources, and prediction latency
  ([#22](https://github.com/HsiangNianian/soon/pull/22)).
- Added native Linux and macOS release smoke tests and one coordinated Cargo
  and PyPI publishing workflow
  ([#23](https://github.com/HsiangNianian/soon/pull/23)).

### Changed

- Predictions now preserve the complete historical command instead of reducing
  the suggestion to an executable name
  ([#13](https://github.com/HsiangNianian/soon/pull/13)).
- Reframed the project and public roadmap around a measurable, local-first
  personal terminal agent
  ([#18](https://github.com/HsiangNianian/soon/pull/18)).
- `soon update` now checks only the configured package registry, compares
  semantic versions, and refuses unsupported AUR or standalone-binary channels
  ([#23](https://github.com/HsiangNianian/soon/pull/23)).

### Maintenance

- Updated `rustls-webpki` to 0.103.13
  ([#2](https://github.com/HsiangNianian/soon/pull/2), by
  [@dependabot](https://github.com/dependabot)).

[Full diff](https://github.com/HsiangNianian/soon/compare/v0.3.0...v0.4.0)
