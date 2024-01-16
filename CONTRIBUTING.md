## Contributing

[fork]: https://github.com/github/twirp-rs/fork
[pr]: https://github.com/github/twirp-rs/compare
[code-of-conduct]: CODE_OF_CONDUCT.md

Hi there! We're thrilled that you'd like to contribute to this project. Your help is essential for keeping it great.

Contributions to this project are [released](https://help.github.com/articles/github-terms-of-service/#6-contributions-under-repository-license) to the public under the [project's open source license](LICENSE.md).

Please note that this project is released with a [Contributor Code of Conduct](CODE_OF_CONDUCT.md). By participating in this project you agree to abide by its terms.

## Prerequisites for running and testing code

We recommend that you install Rust with the `rustup` tool. `twirp-rs` targets stable Rust versions.

## Submitting a pull request

1. [Fork][fork] and clone the repository.
1. Install `protoc` with your package manager of choice.
1. Build the software: `cargo build`.
1. Create a new branch: `git checkout -b my-branch-name`.
1. Make your change, add tests, and make sure the tests and linter still pass.
1. Push to your fork and [submit a pull request][pr].
1. Pat yourself on the back and wait for your pull request to be reviewed and merged.

Here are a few things you can do that will increase the likelihood of your pull request being accepted:

- Write tests.
- Keep your change as focused as possible. If there are multiple changes you would like to make that are not dependent upon each other, consider submitting them as separate pull requests.
- Write a [good commit message](http://tbaggery.com/2008/04/19/a-note-about-git-commit-messages.html).

## Setting up a local build

Make sure you have [rust toolchain installed](https://www.rust-lang.org/tools/install) on your system and then:

```sh
cargo build && cargo test
```

Run clippy and fix any lints:

```sh
cargo fmt --all -- --check
cargo clippy -- --deny warnings -D clippy::unwrap_used
cargo clippy --tests -- --deny warnings -A clippy::unwrap_used
```

## Releasing (write access required)

If you are one of the maintainers of this package then follow this process:

1. Create a PR for this release with following changes:
  - Updated `CHANGELOG.md` with desired change comments and ensure that it has the version to be released with date at the top.
  - Go through all recent PRs and make sure they are properly accounted for.
  - Make sure all changelog entries have links back to their PR(s) if appropriate.
  - Update package version in Cargo.toml.
1. Get an approval and merge your PR.
1. Run ./script/publish from the `main` branch supplying your token and version information.

## Resources

- [How to Contribute to Open Source](https://opensource.guide/how-to-contribute/)
- [Using Pull Requests](https://help.github.com/articles/about-pull-requests/)
- [GitHub Help](https://help.github.com)
