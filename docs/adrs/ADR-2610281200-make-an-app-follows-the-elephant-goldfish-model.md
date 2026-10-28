# ADR-2610281200: Make an app follows the Elephant-Goldfish Model, enforced

**Status:** Accepted. Amends ADR-2610281000 (decisions 2, 3 and 4).
**Date:** 2026-10-28
**Drivers:** The user, after the first build with one reviewer: "wardian make an app has to use the
EGM workflow or whats the point", then "wardian app builder must use the elephant goldfish model".

## Context

- **ADR-2610281000** added `fresh_review` with the lighter options chosen at the start: one reviewer
  with five lenses, the builder deciding trade-offs alone, and the steps only asked for in the
  prompt. A real build followed them, but it was one goldfish, the person never saw the plan, and
  nothing stopped a model that skipped a step.
- **EGM** (egm.dev, github.com/google/elephant-goldfish-model, by Google's EGM team, Apache-2.0):
  the elephant drafts; several goldfish, each in a fresh context with one persona, review; the
  person resolves the findings and approves the spec before it is implemented.
- **Make an app** started each turn from the saved app, so a plan waiting for an answer before the
  first save would have been lost.

## Decision

1. **One goldfish per lens,** each its own request with none of the chat: after Design, scope,
   first-time reader, design and ready to build (after EGM's zoom-out, comprehension, design critic
   and implementation-readiness reviewers); before finish, built as planned and quality checklist.
   They share one cached system prompt and references; each gets the package, `asked` and its one
   lens, and reports findings (must, should, could) and at most two decisions for the user, in at
   most 3,000 tokens. `review_requests` builds them, `review_outcome` puts their reports together
   under each reviewer's name and names any that failed. Wardian runs them at once on the tasks
   port; the hosted builder as steps.
2. **The person approves the plan.** The prompt has five stages: Blueprint, Design (and the design
   review), Approve, Develop, Validate (and the build review). At Approve the builder ends its turn
   with what it will build, what the reviewers changed and each open decision with the option it
   recommends, and asks for "go" or changes. Decisions from the build review are taken as
   recommended and told after the save.
3. **The gates, in the domain** (`domain::studio::Flow`), asked before every tool and told after:
   a new app starts at Plan, where only README.md, suite.json and app.json may be written; a design
   review (even a failed one) moves it to Approval; the person's next message moves it to Build.
   Finish needs a build review (even a failed one) since the person's last message; the fixes after
   it are its resolution and need no other review, because reviewing until nothing is found never
   ends (the first real build reviewed four times before it was stopped). A change to
   a saved app starts at Build, so it is not stopped for approval by the host, but its finish needs a
   build review, and the prompt asks it to plan and ask first when the change is large. While the
   plan waits there is no nudge to finish.
4. **The draft is kept:** files not yet saved stay with the chat between messages
   (`Session::draft`), unless the person stops the turn.

## Consequences

- A new app costs four design goldfish and two build goldfish, of up to 3,000 tokens out each,
  and one more message from the person. The goldfish run at once, so the wait is about one review.
- A model cannot skip the method: a refused call says what to do next.
- The person sees and decides the plan before tokens are spent on code.

## Implementation

`domain::studio` (`Flow`, `Stage`, `Session::{flow, draft}`); `usecases::workshop` (`GOLDFISH`,
the lenses, `review_requests`, `review_outcome`, the prompt); `usecases::studio` (`Api::review`
on the tasks port, `run_tool`'s gates, the kept draft); `wasm/workshop` (`reviewRequest`,
`reviewOutcome`, `flow`); `docs/site/components.md` "Fresh reviews"; `NOTICE.md`.
Tests: `workshop_goldfish_reviews_see_the_package_and_not_the_chat`,
`workshop_flow_gates_code_on_an_approved_plan_and_finish_on_a_build_review`,
`studio_runs_make_an_app_through_the_elephant_goldfish_model`.

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- goldfish flow_gates elephant_goldfish`
