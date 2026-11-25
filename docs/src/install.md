# Install

## Homebrew (macOS and Linux)

```sh
brew install pallewela/tap/bb
brew update && brew upgrade pallewela/tap/bb   # upgrade later
```

The tap covers macOS and Linux on both arm64 and x86_64. Always use the full name
`pallewela/tap/bb`: plain `bb` also matches an unrelated cask.

## Linux packages

Every release has a `.deb` and an `.rpm` for x86_64 and arm64, and the AUR has
`bitbucket-bb-cli-bin`. The package is called `bitbucket-bb-cli` (plain `bb` is an unrelated
Debian package); the command is still `bb`. Replace `X.Y.Z` with the
[latest version](https://github.com/pallewela/bb/releases/latest):

```sh
# Debian, Ubuntu and derivatives (arm64: _arm64.deb)
curl -LO https://github.com/pallewela/bb/releases/download/vX.Y.Z/bitbucket-bb-cli_X.Y.Z-1_amd64.deb
sudo apt install ./bitbucket-bb-cli_X.Y.Z-1_amd64.deb

# Fedora, RHEL, CentOS Stream and derivatives (arm64: .aarch64.rpm)
sudo dnf install https://github.com/pallewela/bb/releases/download/vX.Y.Z/bitbucket-bb-cli-X.Y.Z-1.x86_64.rpm

# Arch Linux and derivatives, with an AUR helper such as yay or paru
yay -S bitbucket-bb-cli-bin
```

The `.deb` and `.rpm` contain the same static binary as the release archives. They don't add a
package repository, so upgrade by installing the next release's package the same way. The `.rpm`
isn't GPG-signed (dnf says it skipped OpenPGP checks); verify it with its `.sha256` or
`gh attestation verify` (below) instead. The AUR package downloads the release archive and checks
its SHA256.

## Cargo

```sh
cargo install bitbucket-bb-cli     # from crates.io; installs the `bb` binary
cargo install --path .             # from a clone of the repository
```

On crates.io the crate is `bitbucket-bb-cli` (`bb`, `bb-cli` and `bitbucket-cli` are other
projects). Building it needs Rust and a C compiler.

## Prebuilt binaries

Every release on the [releases page](https://github.com/pallewela/bb/releases) has an archive per
platform, named `bb-vX.Y.Z-TARGET.tar.gz` (`.zip` on Windows), each with a `.sha256` file:

| Target | Platform |
|---|---|
| `aarch64-apple-darwin` | macOS, Apple silicon |
| `x86_64-apple-darwin` | macOS, Intel |
| `x86_64-unknown-linux-musl` | Linux x86_64, any distro (static) |
| `aarch64-unknown-linux-musl` | Linux arm64, any distro (static) |
| `x86_64-pc-windows-msvc` | Windows x86_64 (`.zip` with `bb.exe`) |

Linux binaries are statically linked (musl), so one file works on every distro, including Alpine
and scratch containers.

### Verify a download

Each archive and Linux package has a `.sha256` file, and every release has a signed
[build provenance](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations)
attestation: it proves the archive was built by this repository's release workflow, from the
tagged commit. With the GitHub CLI:

```sh
gh attestation verify bb-vX.Y.Z-TARGET.tar.gz --repo pallewela/bb
```

Releases after v0.2.11 have it. The same attestation is attached to each release as
`bb-vX.Y.Z.intoto.jsonl`, for checking without fetching it from GitHub: add
`--bundle bb-vX.Y.Z.intoto.jsonl` to the command.

On Windows, unzip `bb.exe` into a folder on your `PATH`. The binary is unsigned, so SmartScreen may
warn the first time it runs. There is no Scoop or winget package yet.

Check the install:

```sh
bb --version
```

Next: [authenticate](auth.md).
