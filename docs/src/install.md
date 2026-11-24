# Install

## Homebrew (macOS and Linux)

```sh
brew install pallewela/tap/bb
brew update && brew upgrade pallewela/tap/bb   # upgrade later
```

The tap covers macOS and Linux on both arm64 and x86_64. Always use the full name
`pallewela/tap/bb`: plain `bb` also matches an unrelated cask.

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

On Windows, unzip `bb.exe` into a folder on your `PATH`. The binary is unsigned, so SmartScreen may
warn the first time it runs. There is no Scoop or winget package yet.

Check the install:

```sh
bb --version
```

Next: [authenticate](auth.md).
