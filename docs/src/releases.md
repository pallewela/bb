# Releases and platforms

## How releases happen

Every push to `main` that passes CI is released automatically by
[`.github/workflows/release.yml`](https://github.com/pallewela/bb/blob/main/.github/workflows/release.yml).
The patch version is bumped by default. Put one of these in the head commit's subject to change that
(the same text in the body is ignored; only `BREAKING CHANGE` counts anywhere in the message):

| Marker | Effect |
|---|---|
| `[bump:minor]` | Bump the minor version |
| `[bump:major]` or `BREAKING CHANGE` | Bump the major version |
| `[skip release]` | Don't release this push |

See [CONTRIBUTING.md](https://github.com/pallewela/bb/blob/main/CONTRIBUTING.md#commit-messages-and-releases)
for how this works with squash merges.

Pull requests build every target too (without publishing), so a change that breaks one platform's
build is caught before it reaches `main`.

## Targets

Each release publishes one archive per target, built and checked on a native GitHub runner:

| Target | Platform | Archive | Check before publishing |
|---|---|---|---|
| `aarch64-apple-darwin` | macOS, Apple silicon | `.tar.gz` | runs `bb --version` |
| `x86_64-apple-darwin` | macOS, Intel (cross-built on arm64) | `.tar.gz` | `lipo -archs` is `x86_64` |
| `x86_64-unknown-linux-musl` | Linux x86_64, any distro | `.tar.gz` | runs `bb --version`; no shared libraries |
| `aarch64-unknown-linux-musl` | Linux arm64, any distro | `.tar.gz` | runs `bb --version`; no shared libraries |
| `x86_64-pc-windows-msvc` | Windows x86_64 | `.zip` (`bb.exe`) | runs `bb --version` |

Archives are named `bb-vX.Y.Z-TARGET.{tar.gz,zip}`, contain the binary, `LICENSE` and
`README.md`, and come with a `.sha256` file. The release also has a signed build provenance
attestation for all archives (`bb-vX.Y.Z.intoto.jsonl`); see
[Verify a download](install.md#verify-a-download). All of them are published in one release
call, so a release is never partial.

Linux binaries are static (musl): TLS is `rustls` and the keychain (Secret Service) is reached
over D-Bus with pure-Rust `zbus`, so no system library is needed.

Not built: Windows arm64, 32-bit targets, FreeBSD.

## Distribution channels

- **GitHub Releases:** <https://github.com/pallewela/bb/releases>.
- **Homebrew:** after a release, the formula in
  [pallewela/homebrew-tap](https://github.com/pallewela/homebrew-tap) is updated with each
  archive's SHA256 (needs the `TAP_REPO_TOKEN` secret).
- **crates.io:** the crate `bitbucket-bb-cli` is published with the same version, using crates.io
  trusted publishing (GitHub OIDC, no stored token), when the repository variable
  `CRATES_IO_PUBLISH` is `true`. A version that is already on crates.io is skipped, so re-runs are
  safe.
- **Windows:** install from the `.zip` by hand. No Scoop or winget package yet; the binary is
  unsigned.

## This documentation site

This book lives in `docs/` and is built with [mdBook](https://rust-lang.github.io/mdBook/). Pushes
to `main` that change `docs/**` rebuild and deploy it to GitHub Pages
(`.github/workflows/pages.yml`). To preview locally:

```sh
mdbook serve docs --open
```
