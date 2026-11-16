vssh release plan (drafted by Hermes 16/11, after PR #1 merge)
===============================================================

Goal: tag v0.1.0, ship GitHub Releases with static binaries + .deb (amd64/arm64),
keep install.sh as the sudo-less path.

## Deliverables
1. `.github/workflows/release.yml` — on tag push `v*`: build musl static
   x86_64 + aarch64, run tests (on x86_64), pack:
   - `vssh-<target>.tar.gz` (binary)
   - `vssh_<ver>_<amd64|arm64>.deb` via plain dpkg-deb (no cargo-deb needed)
   Attach all to the GitHub Release with auto-generated notes.
2. `[package.metadata.deb]` in Cargo.toml so `cargo deb` also works locally.
3. install.sh unchanged (already bilingual, release-binary aware).

## Install paths after release
- .deb: `sudo apt install ./vssh_0.1.0_amd64.deb` (or `dpkg -i`), no network build.
- Static binary: download tar.gz, drop in ~/.local/bin.
- One-liner: `curl -fsSL .../install.sh | sh` (unchanged).

## Steps
- [ ] Commit release.yml + Cargo.toml deb metadata on a branch, PR, merge
- [ ] `git tag v0.1.0 && git push origin v0.1.0` → workflow builds + publishes Release
- [ ] Verify Release assets: 2 tar.gz + 2 .deb; `apt install` the amd64 .deb in a container
- [ ] Paste release URL into README badges/buttons (optional)
- [ ] Optional later: apt repo via gh-pages (`deb https://duclong06.github.io/vssh ./ main`)
      and cargo-dist if we want homebrew/nixpkgs channels too

## Not planned
- AppImage / rpm: .deb + static tar.gz cover our Linux cases; revisit on request.