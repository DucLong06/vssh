# Task: CI/CD + full test suite + security tests for vssh

After the main implementation is complete (RESULT.md exists, cargo build/test/clippy green), add:

## 1. GitHub Actions CI (`.github/workflows/ci.yml`)
- Triggers: push to any branch, PRs to main.
- Job `build-test` (ubuntu-latest, stable Rust toolchain from rust-toolchain.toml if present):
  - `cargo fmt --check`
  - `cargo build --release`
  - `cargo test` (all tests)
  - `cargo clippy -- -D warnings`
- Job `audit` (security):
  - `cargo audit` (via rustsec/audit-check action or cargo install cargo-audit)
  - `cargo deny check advisories` if cargo-deny config added
- Cache with `Swatinem/rust-cache@v2`.
- Keep CI fast: single job matrix is fine; do not add cross-compile jobs.

## 2. Security-focused tests (in-crate, `#[cfg(test)]`, plus integration tests in `tests/`)
Cover at least:
- Vault: wrong passphrase rejected, tamper detection (bit flips in magic/salt/nonce/ciphertext/tag), file mode 0600, fresh nonce per save, refuses overwrite, malformed/truncated file rejected, empty passphrase rejected. (Existing tests exist — keep and extend if gaps.)
- ssh-config parser: injection safety — a Host line with weird tokens, IdentityFile with `~` expansion, comments, Include lines must not leak into wrong hosts; parse errors must not panic.
- Host key handling: unit-test the policy logic where possible (unknown vs known vs mismatch).
- store/db: metadata SQL is parameterized everywhere (no string-built queries); test add/edit/delete host metadata.
- No secret ever appears in Debug/Display output or logs: add a test asserting that Vault's Debug output and error messages never contain a stored secret.

## 3. Verification
- Run the full CI set locally for real: fmt --check, build, test, clippy, cargo audit. Paste real outputs into the final summary.
- Commit in small logical commits. Update README badge section placeholder if needed (do not invent results).

## Rules
- Do not rewrite existing history. Follow repo conventions (small commits, clear messages).
- Do not fabricate any output. If cargo-audit is unavailable offline, say so and leave the CI job to run it on GitHub.
