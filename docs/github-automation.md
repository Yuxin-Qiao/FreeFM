# GitHub Automation

This repository keeps merge authority deterministic and human-controlled. The
normal flow is:

```text
Contributor PR
  -> deterministic CI and security scans
  -> trusted-base AI Review over PR diff data
  -> human review / CODEOWNERS
  -> main Ruleset
  -> merge
  -> optional tag-driven Release workflow
```

## Current components

- `CI`: Rust format, tests, clippy, release build, MSRV, RustSec, gitleaks,
  workflow pinning, shell/plist checks, and release smoke tests.
- `AI Review`: runs from `pull_request_target` but checks out only the trusted
  base commit. It downloads the PR diff and file list as data, so PR head code
  is never executed with model secrets or write-capable tokens.
- `CodeQL`: scans trusted `main` pushes and a weekly scheduled build for Rust.
  It intentionally does not execute fork PR code with `security-events: write`.
- `Dependabot`: weekly Cargo and GitHub Actions updates. Patch/minor updates are
  grouped; major updates remain explicit PRs.
- `Codecov`: the CI coverage job produces an LCOV report with the real Rust test
  suite and uploads it as an informational quality signal. It is not a merge
  authority and has no invented coverage threshold.
- `CodeRabbit`: repository-side review policy for non-draft PRs. It focuses on
  correctness, regressions, security, API compatibility, lifecycles, and tests;
  it does not request changes or replace deterministic CI and human review. The
  existing AI verifier remains the acceptance/security assistant; CodeRabbit is
  advisory code review, so neither AI tool is merge authority.
- `Release`: tag-driven native artifacts, WorkBuddy package, SBOM, attestations,
  security gates, and a protected `release` environment.

## Required checks

The `main` Ruleset should require these exact check contexts:

```text
ai-review
rust (ubuntu-latest)
rust (macos-latest)
msrv
rustsec
secrets
verification
```

AI is an enhancement layer. It must not replace deterministic CI, security
scans, or human review.

## External setup required

1. Keep the repository secrets `AI_REVIEW_API_KEY`, `AI_REVIEW_ENDPOINT`, and
   `AI_REVIEW_MODEL` configured. The workflow treats endpoint failure as
   fail-open and reports the skip; it never prints the key.
2. Keep the active `AI Review gate` Ruleset targeted at `main`, require the
   checks above, strict up-to-date branches, and at least one independent human
   approval (`required_approving_review_count: 1`). Stale approvals must be
   dismissed on push and all review threads must be resolved before merge.
3. Configure required reviewers for the `release` environment in GitHub
   Settings so a tag cannot publish without the documented human Go/No-Go.
4. Install the CodeRabbit GitHub App for `Yuxin-Qiao/FreeFM`. The committed
   `.coderabbit.yaml` is the repository-side policy; the app is still an
   external installation.
5. Activate `Yuxin-Qiao/FreeFM` in Codecov. Add a `CODECOV_TOKEN` repository
   secret if Codecov requires authentication for protected `main` uploads;
   fork PRs must never receive that secret.
6. CodeQL alerts and Dependabot security/update PRs are GitHub-native and need
   no third-party SaaS installation.

## Intentionally not added

- Renovate: Dependabot already covers the only real ecosystems here (Cargo and
  GitHub Actions), so adding a second dependency bot would duplicate PRs.
- Release Please/Changesets: the existing tag-driven release workflow and
  `RELEASING.md` require an explicit human Go/No-Go and prohibit automatic
  releases; adding an automated release PR bot would violate that policy.
- stale, backport, labeler, merge queue, Mergify, Probot, self-hosted services,
  and databases: the repository has one open issue, few branches, and no
  maintained release branches or contention that justifies them.

## Security boundaries

- PR CI uses `pull_request`, read-only permissions, and no secrets.
- The write-capable AI workflow uses trusted base code only; diff and metadata
  are untrusted strings passed to the verifier, never shell source.
- Release is tag-driven, uses least-privilege job permissions, and publishes
  only after security and smoke gates.
- Codecov and CodeRabbit are advisory integrations. Their tokens and app
  permissions must never be made available to `pull_request` jobs from forks.
- Scheduled FreeFM synchronization remains `freefm sync --quiet` with zero LLM
  calls; the AI workflow exists only for GitHub PR review.
