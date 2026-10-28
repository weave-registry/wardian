# ADR-2610281000: fresh reviews check an app's plan and its build, the Elephant-Goldfish way

**Status:** Accepted
**Date:** 2026-10-28
**Drivers:** The user: "we need to adopt this in helping wardian design apps both on wardian-studio
and wardian itself https://egm.dev/". Chosen with the user: a review of the plan after Design and of
the files before finish; one request with several lenses, not one per lens; the builder decides a
trade-off itself and tells the user after finish; the Claude Code skill uses EGM for suites and
apps with AI. Then: "make sure we give attributions to the EGM team".

## Context

- **EGM** (the Elephant-Goldfish Model, egm.dev, github.com/google/elephant-goldfish-model, by
  Google's EGM team, Apache-2.0, after Dave Rensin's essay): an agent with the whole context (the elephant)
  drafts a design, and reviewers with none of it (goldfish) read only the document and its sources,
  so they see the assumptions the author can no longer see. A person settles the trade-offs.
- **Make an app** (ADR-2610111770) already plans before it builds: Blueprint and Design in the
  README, then Develop, then Validate. But the builder checks its own plan and its own code, with
  the whole chat in mind, and it finds what it expects to find.
- **The skills agent** (ADR-2610131000) showed the shape: a tool the host dispatches as a separate
  model request, built by pure functions in `workshop.rs` that both Wardian and the hosted builder
  (through `wasm/workshop`) run.

## Decision

1. **A `fresh_review` tool,** with `stage` ("design" or "build") and `asked`, the user's request in
   their words. The host sends it as its own request (`workshop::review_request`): the reviewer's
   method, the spec, the component notes and the Validate checklist as cached references, and one
   message holding `asked` and the package: nothing of the chat. A design review sees the file
   list, `README.md` and the manifests; a build review also sees every text file except the
   library's copies under `ui/`. No README, or another stage, is the tool's error. No tools, at most
   6,000 tokens out.
2. **Five lenses in one request:** scope (the smallest app that does the job asked), inputs and
   forms, states and behaviour, ready to build (or built as planned, with the checklist item by
   item), and reads clearly. The review comes under fixed headings: Verdict, Findings (each
   **must**, **should** or **could**, the lens, where, the fix, at most twelve), Trade-offs (at most
   three, each with the side it recommends) and Looks right. `review_outcome` turns it into the
   tool's result; an empty or refused review is an error and the builder reviews by itself.
3. **When:** the prompt calls it at the end of Design, before any code, and in Validate before
   check and finish; checklist item 12 checks it ran and its musts are fixed. A change calls the
   design review when it changes the inputs, results or layout, and the build review when it changes
   more than a few lines; a `[browser test]` fix calls neither.
4. **The person stays in charge, without being asked mid-build.** The builder fixes every must and
   should unless it goes against what the user asked, takes the recommended side of a trade-off, and
   after finish tells the user each such choice in one plain sentence, so they can ask for the
   other.
5. **Both hosts:** Wardian's Make an app runs it in its loop on the builder's meter and daily cap
   (`Api::review`). The hosted builder gets `reviewRequest` and `reviewOutcome` from
   `wasm/workshop` and runs it as its own step, like the skills agent (its ADR in wardian-web).
6. **Attribution:** the method is EGM's, and the review's lenses are modelled on its reviewers
   (zoom-out, design critic, implementation readiness, comprehension); the prompt's text is
   Wardian's own, and no EGM text is copied. `NOTICE.md` credits EGM, its team, its repository, its
   licence and the essay; so do the "Fresh reviews" docs, the app-factory skill, the code and the
   changelog.
7. **Claude Code:** the app-factory skill has a goldfish review for suites and apps with AI, with the
   EGM plugin's `egm-brainstorm` and `egm-review` when it is installed and a fresh-context subagent
   when it is not. A page app with one job keeps the fast path.

## Consequences

- The plan is checked before code is written, when a gap costs least, and the code is checked
  against the plan by a reader who did not write either.
- A new app costs two more requests, of up to 6,000 tokens out each, and a few more builder steps
  to fix what they find. Changes cost them only when they are more than small.
- One request with five lenses is less independent than five goldfish; it can be split per lens
  later without changing the tool.
- The reviewer knows only what the package and `asked` say, so a requirement said in the chat and
  never written down shows up as a finding, which is the point.

## Implementation

`usecases::workshop` (`REVIEW_TOOL`, `REVIEW_AGENT`, the prompt, `review_request`,
`review_outcome`); `usecases::studio` (`Api::review`, `run_tool`); `wasm/workshop` (`reviewRequest`,
`reviewOutcome`); `docs/site/components.md` "Fresh reviews", `docs/site/ai.md`;
`skills/wardian-app-factory/SKILL.md` and `.claude/skills`, `NOTICE.md`.
Tests: `workshop_fresh_review_sees_the_package_and_not_the_chat`,
`studio_dispatches_a_fresh_review_without_the_chat`.

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- workshop_fresh_review studio_dispatches_a_fresh`
