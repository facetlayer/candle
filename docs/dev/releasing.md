
# Releasing #

How to release a new version of Candle:

 1. Bump `version` in `rust/Cargo.toml` ([package]) and update CHANGELOG.md.
 2. `cargo build --release --manifest-path rust/Cargo.toml` to refresh Cargo.lock, then commit.
 3. `git tag vX.Y.Z && git push origin vX.Y.Z`.

 The release workflow builds the single `candle` binary for macOS + Linux (x86_64 and arm64),
 publishes a GitHub Release with SHA256SUMS, and updates the Homebrew formula in
 facetlayer/homebrew-tap.
 The Homebrew step needs a `HOMEBREW_TAP_TOKEN` repo secret and is skipped without it.

