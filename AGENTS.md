# RustCINRAD Agent Guidelines

This repository follows the guidance below when making changes.

## Development commands

**Official Rust documentation**
- [The Rust Reference](https://doc.rust-lang.org/reference/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/) (unsafe code)
- [Rust 2024 Edition Guide](https://doc.rust-lang.org/edition-guide/rust-2024/)
- [The Cargo Book](https://doc.rust-lang.org/cargo/)
- [Standard library docs](https://doc.rust-lang.org/std/) and [release notes](https://doc.rust-lang.org/releases.html)

**Books & guides**
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/) — Nicholas Nethercote
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) — rust-unofficial
- [Rust Atomics and Locks](https://marabos.nl/atomics/) — Mara Bos
- [Effective Rust](https://effective-rust.com/) — David Drysdale

**Tooling**
- [Clippy lint documentation](https://rust-lang.github.io/rust-clippy/)
- [Miri](https://github.com/rust-lang/miri)

- Check the code with `cargo check --workspace --all-features --tests`.
- Test the code with `cargo test`. Do not pass `--all-features`.
- Format Rust code with `cargo fmt --all`.

## Commit messages

- Use the Conventional Commits format:
  `<type>[optional scope]: <description>` where `<type>` is one of
  `feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `test`, `build`, `ci`, or `revert`.
- For documentation fixes use `docs: fix <description>`.
- Prefer small, focused commits. For larger changes, use multiple commits with clear messages.

## Recommendations

- Run test, format and clippy before submitting a pull request and ensure all CI checks pass.
- Keep documentation and examples in sync with code changes.
- Use feature branches for separate tasks.
- Open issues and pull requests through GitHub for discussion and review.
- Always rebase your branch before when editing an open pull request to keep the history clean.