# Changelog

All notable changes to soon are documented here.

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
