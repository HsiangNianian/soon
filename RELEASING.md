# Releasing soon

## v0.5 beta support contract

The interactive beta is supported on Zsh on native Linux and macOS runners. The on-demand CLI and Python wheels are built for the targets listed in `.github/workflows/release.yml`, but shell parsers other than Zsh do not yet imply supported interactive integration.

| Channel | v0.5 beta status | Update source |
|---|---|---|
| Cargo package `soon` | Supported | crates.io package metadata |
| PyPI package `soon-bin` | Supported | PyPI package metadata |
| AUR package `soon` | Unsupported; the old package is not part of the beta release | None; `soon update` refuses the channel |
| Standalone binaries | Unsupported until a signed binary artifact workflow exists | None; `soon update` refuses the channel |

Cargo and PyPI must be published from the same `vX.Y.Z` tag. `Cargo.toml` is the version source; `Cargo.lock` must match it, while `pyproject.toml` obtains the wheel version dynamically from Cargo metadata.

## Pull request gate

Every pull request runs:

- formatting, tests, Clippy with warnings denied, and a CLI version smoke;
- the commit-bound proof checks on Linux;
- the native beta suite on Linux and macOS;
- an isolated Zsh install-to-uninstall smoke covering init, manual prediction, Next-step, Repair, acceptance, inspect, clear, disable, and uninstall;
- Cargo packaging and the full PyPI wheel/sdist build matrix.

## Release sequence

1. Confirm `master` is clean and every artifact-blocking code or documentation issue is closed or included in the release. A research issue may remain open only when it explicitly depends on the release artifact; record that relationship in both issues.
2. Add one `CHANGELOG.md` section and update `Cargo.toml` plus `Cargo.lock` to the same version.
3. Run the repository checks and `cargo package --locked`.
4. Merge the release commit.
5. Create and push one annotated `vX.Y.Z` tag on the merged commit.
6. The `Release Cargo and PyPI` workflow validates tag/version/changelog equality, builds every artifact, publishes Cargo first, then publishes the same artifacts to PyPI.
7. Publish the GitHub Release from the same changelog section.
8. Verify the exact version on crates.io and PyPI before closing the release issue. Close the milestone only after any downstream research issues are complete.

Never force-rewrite a published release tag. If a registry publication is defective, stop the other release work, record the failure, and use registry-native yanking rather than reusing the version.
