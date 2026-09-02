---
name: update-deps
description: Use when the scheduled dependency workflow asks Copilot to finish a Cargo or GitHub Actions update in this repository.
user-invocable: true
---

# Update dependencies

Finish one deterministic dependency update without crossing the workflow's read-only trust boundary.

## Repository context

- The default branch is `main`.
- Rust is pinned by `rust-toolchain.toml` to 1.94.0.
- This is a Cargo workspace with a tracked root `Cargo.lock`.
- CI installs protoc with `script/install-protoc`, then runs `make build`, `make test`, and `make lint`.
- Protobuf output is generated in `OUT_DIR`; it is not checked in. Do not claim `script/install-protoc` regenerates repository files, and do not add generated protobuf output.
- The automation maintains one reserved draft PR per ecosystem: `automation/cargo-dependencies` and `automation/github-actions-dependencies`.

## Trust boundary

The `generate` job has read-only repository access. It runs the native updater, captures checks, uploads an immutable post-updater baseline, and then invokes Copilot. The separate `apply` job has write access but never executes agent output.

Do not commit, push, create or edit pull requests, change workflow permissions, or run schedules. The apply job owns all GitHub writes.

For Cargo, worktree changes are limited to `Cargo.toml`, `Cargo.lock`, `crates/**`, and `example/**`. For GitHub Actions, only the revision and tracked ref on an existing remote `uses:` line may change; the action repository, path, quoting, file mode, and every other line must remain identical.

## Workflow

1. Read `/tmp/dependency-update-context/native-update.log` and every `*-initial.log` and `*-initial.exit` file.
2. Inspect the complete ecosystem delta, not only the first failing package or action.
3. Fix compatibility failures for Cargo within the allowlist. Do not revert dependency updates just to make checks pass unless the update is unsafe and the PR should be a no-op.
4. Run the ecosystem validation below.
5. Write a one-line PR title to `/tmp/dependency-pr-title.txt` and a concise Markdown body to `/tmp/dependency-pr-body.md`. Write exactly `noop` as the title when no safe dependency update remains; the body is then optional.

## Cargo updates

`script/update-cargo-dependencies` runs native `cargo update` first. Also inspect direct dependency constraints in every workspace manifest for available releases, including major versions that `cargo update` cannot select without a manifest change. Use Cargo and crates.io source metadata rather than guessing versions.

Use `cargo tree -d` and the compiler output to identify incompatible duplicate major versions. The September 2026 combined dependency PR is the reference failure mode: `prettyplease` 0.3 accepts `syn` 3 syntax trees while `twirp-build` used `syn` 2. A useful update adapts the manifest and consumer together instead of pinning the old dependency or adding conversion fallbacks.

Run:

```bash
make build
make test
make lint
```

The workflow installs the repository's pinned protoc before invoking you. Protobuf dependency changes require normal build/test coverage only because generated files live in `OUT_DIR`.

## GitHub Actions updates

`script/update-github-actions` inventories `.github/workflows/**/*.{yml,yaml}` and `.github/actions/**/*.{yml,yaml}`, resolves each tracked tag or branch, and replaces the action revision with its full 40-character commit SHA. The comment after each pin is the tracked ref for the next run. Inspect upstream releases for newer release lines too; to adopt one, change only the tracked ref comment on an existing `uses:` line and rerun `script/update-github-actions` to resolve its SHA.

Do not change action owners, repositories, paths, workflow behavior, permissions, triggers, or file modes. Confirm:

```bash
script/update-github-actions --check
git diff --check
```

The PR body should identify every updated action and tracked ref.

## PR metadata

Use `Update Cargo dependencies` or `Update GitHub Actions dependencies` as the title unless a more specific focused title is justified. The body should explain material compatibility changes and risks without restating every diff line. Do not include a `Co-authored-by` trailer in the body.

## Conflicts, security, and rollout

`script/check-dependency-pr-conflicts` blocks a run while a Dependabot PR, a combined Dependabot PR, a human-owned reserved branch, or a non-draft reserved PR overlaps the ecosystem. Never create another dependency PR to work around that refusal.

Dependabot version PRs are disabled only as this replacement lands; Dependabot security updates remain enabled. Rollout order is: exclude `github/twirp-rs` from the external Blackbird combiner, resolve or close the existing #343-#348 dependency PRs, then allow these schedules to create their reserved drafts.

Draft PRs still require human review, and required PR CI remains approval-required. `GITHUB_TOKEN` is intentionally used for Copilot and PR creation, but its PR events cannot drive automatic post-PR CI/fixup. That remains blocked until a repository-scoped GitHub App or PAT is provisioned.
