# Feedback and contributing

`bb` is developed in the open at **<https://github.com/pallewela/bb>**. Bug reports, feature
requests and pull requests are all welcome.

## Get bb

- Install a release with Homebrew, a Linux package (`.deb`, `.rpm`, AUR), Cargo or a prebuilt
  binary: see [Install](install.md).
- Every release, with its changelog and checksums, is on the
  [releases page](https://github.com/pallewela/bb/releases).
- The source code is in the [GitHub repository](https://github.com/pallewela/bb):
  `git clone https://github.com/pallewela/bb.git`.

## Report a bug

Open an [issue](https://github.com/pallewela/bb/issues/new) with:

- the output of `bb --version` and your OS;
- the exact command you ran;
- what you expected and what happened instead (paste the output).

Remove tokens, emails and private repository names from anything you paste. `bb auth status`
without `-t` is safe to paste. Check the [existing issues](https://github.com/pallewela/bb/issues)
first in case it's already reported.

Problems with Bitbucket Cloud itself (outages, API behavior, account or token problems) are for
[Atlassian Support](https://support.atlassian.com/bitbucket-cloud/), not this project.

## Request a feature or enhancement

Open an [issue](https://github.com/pallewela/bb/issues/new) describing what you want to do and
why. `bb` follows `gh`, so if `gh` has the equivalent command or flag, link to it. Questions and
how-to requests also go in issues; say it's a question.

## Report a security vulnerability

Don't open a public issue. Report it privately through a
[GitHub security advisory](https://github.com/pallewela/bb/security/advisories/new). The
[security policy](https://github.com/pallewela/bb/blob/main/SECURITY.md) covers what's in scope
and what to include.

## Contribute code or documentation

1. For anything bigger than a small fix, open an issue first so we can agree on the behavior.
   [PRD.md](https://github.com/pallewela/bb/blob/main/PRD.md) lists where and why `bb` differs
   from `gh`.
2. Fork the repository and make your change on a branch. You need a stable Rust toolchain
   (1.88 or newer) and nothing else to build and test:

   ```sh
   cargo build
   cargo test
   cargo fmt
   cargo clippy --all-targets -- -D warnings
   ```

3. Add or update tests for what you changed. A change to the CLI also updates the
   [agent skill](agent-skill.md) in the same pull request.
4. Open a [pull request](https://github.com/pallewela/bb/pulls) with a
   [conventional commit](https://www.conventionalcommits.org/) title, such as
   `fix(pr): ...` or `docs: ...`. CI runs the tests on macOS, Linux and Windows.

[CONTRIBUTING.md](https://github.com/pallewela/bb/blob/main/CONTRIBUTING.md) has the full
details: the test suites, the mock API the CLI tests use, coverage, and how commit titles pick
the release version.

This documentation lives in [`docs/src`](https://github.com/pallewela/bb/tree/main/docs/src) in
the same repository. Every page has an edit button (top right) that opens it on GitHub.

`bb` is maintained in spare time, so replies may take a few days.
