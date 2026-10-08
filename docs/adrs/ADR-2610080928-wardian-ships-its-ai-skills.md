# ADR-2610080928: Wardian ships its AI skills

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user asked to "make sure we have embedded skills for AI development we ship with
wardian". Today the one skill, `wardian-app-factory`, exists only in this repository's
`.claude/skills/`, and it assumes a checkout: `target/release/wardian`, `SPEC.md` in the repo root,
`apps/loan-planner`. Someone who installs Wardian and builds apps with an AI coding assistant gets
none of it.

## Context

Wardian is one binary that carries everything it needs: its pages, the component library, the
templates and, since ADR-2610080903, the documentation site. A skill is a folder: a `SKILL.md` with
a name and a description in its front matter, and files it may read or run. Claude Code finds skills
in `<project>/.claude/skills/<name>/`. A skill that names files outside its folder only works where
those files exist.

## Decision

1. **The skills are part of the program.** Their source is `skills/<name>/` in the repository. The
   program builds every file in it in, as it does the templates.
2. **Two skills.**
   - `wardian-app-factory` builds a package from a description and proves it works (the existing
     skill, made to work outside a checkout).
   - `wardian-app-doctor` finds out why an app fails: a check error, a kernel fault, a contract
     mismatch, a capability that resolves to `null`, a page that the sandbox blocks.
3. **Each skill carries its references.** On install, Wardian adds `references/` to each skill:
   SPEC.md and the docs pages the skill needs, from the copies built in. So a skill works offline,
   in any folder, and always matches the Wardian that installed it.
4. **`wardian skills [FOLDER]`** writes the skills into `FOLDER/.claude/skills/` (default: the
   current folder). Like `wardian add`, it keeps a file that is already there unless given
   `--force`, and says which it kept. `--list` names the skills.
5. **The skills find Wardian themselves.** They run `wardian` from `PATH`; in a checkout they use
   `target/release/wardian`; `WARDIAN_BIN` overrides both. The smoke test does the same.
6. **This repository uses what it ships.** `.claude/skills/` here is the output of `wardian skills .`.
   A test fails when it differs, so the skill the project uses and the skill users get are one.

## Consequences

- Anyone with Wardian gets the same AI tooling with one command, at the version they run.
- The binary grows by the skills and nothing else: the references are the docs already built in.
- A change to a skill, or to a docs page a skill carries, needs `wardian skills . --force` before
  commit, or the test fails.
- `wardian-app-factory` no longer assumes a checkout, so its text names `wardian` instead of
  `target/release/wardian`.

## Implementation

- `skills/wardian-app-factory/` (SKILL.md, scripts/smoke.js), `skills/wardian-app-doctor/SKILL.md`.
- `src/ports/assets.rs` (`skills()`), `src/adapters/secondary/embedded_assets.rs` (the files),
  `src/usecases/skills.rs` (the install set: skill files plus references),
  `src/adapters/primary/cli.rs` (`wardian skills`).
- `.claude/skills/` regenerated; `src/tests.rs`: the freshness test.
- `docs/site/ai-skills.md` and its line in `DOCS`; `docs/site/cli.md`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release skills_`

Rerun by `hexa adr gates`.

## References

- ADR-2610080903 (the docs site, built in the same way), ADR-2610080900 (a suite is made of small parts)
