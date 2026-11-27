# Contributing

## How work happens

Work happens on branches, with pull requests into `main`; nobody pushes to `main` directly. CI runs
on each pull request:

- `cargo fmt --check` and `cargo clippy -D warnings` (no warnings allowed);
- `cargo test` on Linux, Windows and macOS;
- a coverage report, posted on the pull request;
- the book's build and link check.

Run the same locally before pushing ([Testing and coverage](testing.md)).

## A pull request

- **One change per pull request**, with a description of what it does and how it was tested.
- **Tests** for new behavior, checked by breaking the code on purpose to see them fail.
- **Docs** in the same pull request: the book's page for anything users see, a line in
  `CHANGELOG.md` under the version being built, and `docs/spec.md` when a decision or plan changes.
- **Pictures** taken again (`record_docs_screenshots`) for what changed, and only that: the sim's
  values move, so a picture taken again differs a little even when nothing changed, and can come out
  worse. Look at each one before committing.

## How the code is written

raptaboard is read by students who are new to programming, so:

- Every public item has a doc comment in plain language, with units ("seconds", "meters"), and a
  short doctest where a student would call it.
- Each file starts with a `//!` comment saying what it is.
- Names follow Elastic's exactly where robot code or layout files depend on them (`.type` strings,
  `/Elastic/` topics, layout keys), so Elastic layouts open unchanged.
- In code and comments, "server" is the NetworkTables side (the robot is only the usual server; it
  can be the sim) and "robot" is robot code itself.
- `unsafe` isn't allowed anywhere (one Windows call lives in its own small crate, `win-console`).
- Dependencies are pinned to versions that have been out for at least two weeks.

## Using Claude Code

`CLAUDE.md` at the repo's root has the same rules, plus where everything is, for Claude Code. Keep it
up to date along with `docs/spec.md`, so the next session can pick up from the repo alone.
