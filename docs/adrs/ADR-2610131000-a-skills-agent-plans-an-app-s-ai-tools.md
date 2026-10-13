# ADR-2610131000: a skills agent plans an app's AI tools, and the AI stays out of sight

**Status:** Accepted
**Date:** 2026-10-13
**Drivers:** The user: "When an app is integrated in an ai pipeline we need to ensure it has proper
skills and tools to do its work. During app generation dispatch a skills agent to research the best
way to add tools to the app", then "tool management should be behind a separate component for each
app hidden from the application view". Chosen with the user: a tool loop inside the app on today's
`claude:sample` (no change to the format), in both builders, and only when the app uses AI.

## Context

- **An app's AI today:** `claude:sample` is a prompt in and text or JSON out (SPEC 6.6): no tool
  use, at most 60,000 characters, `null` without Claude. An app that needs its AI to look things up
  writes its own prompt and loop, each time differently, and nothing checks the model's calls.
- **Who uses it:** only suite apps can (a page app declares no capabilities). A suite app with no
  `slot` already runs out of sight (SPEC 6.2).
- **Make an app:** one builder prompt and toolset (`workshop.rs`) serves Wardian's own Make an app
  (`studio.rs`) and the hosted builder (`wasm/workshop`). The builder decided the AI's design by
  itself, in the middle of writing the app.

## Decision

1. **The `agent` component** (`static/ui/agent.js`): `WardianUI.agent({sample, skill, tools,
   answer, maxSteps, modelTier, onStep})` runs a tool loop over `sample.json`. Each step the model
   sees the skill, the tools (name, description, input schema), the task and the results so far,
   and replies with `{"tool", "input"}` or `{"answer"}`. The loop checks the input against the
   tool's schema before running it; an unknown tool, bad input or a tool that throws goes back to
   the model as an error; an answer that misses its schema is sent back once to be fixed. It stops
   at the answer or after `maxSteps` (`max_steps`), keeps every prompt under the limit by shortening
   older results, and fails with `unavailable`, `bad_reply` or `claude:sample`'s own codes. Results
   and the task are marked as data, never instructions. `agent.ask` is `run` that never throws:
   `{answer, steps}` or `{error: {code, message}, steps}`, because a rejection loses its code on
   the way through `ctx.call`.
2. **The AI is its own out-of-sight app.** An app with AI keeps its skill, its tools and the loop in
   one suite app named `ai` with no slot, the only app with `claude:sample`. It provides a method
   per job, such as `ask`, at once in `init`, waiting for the capability inside the method so an
   early call finds it; the apps people see call it and show the answer. `add_components` (and
   `wardian add`) lists `ui/agent.js` in that app's scripts only, never in the suite's, and says
   what to add when there is no `ai` app.
3. **The skills agent.** A new builder tool, `research_ai_tools`, with the AI's purpose, what it
   works from, what it may and must never do, and its place in a pipeline. The host dispatches it
   as a separate model request (`workshop::skills_request`): its own system prompt (a method:
   tools or a single request, the domain method in the skill, the fewest compact read-only tools,
   proposals instead of actions, failure and untrusted data, the pipeline), the spec and the agent
   component as cached references, the brief, and the package's README, manifests and AI code. No
   tools, at most 8,000 tokens out. It returns a plan under fixed headings (Verdict, Skill, Tools,
   Answer, the ai app, Safety and limits, Without Claude, Pipeline, Tests), which becomes the tool's
   result (`skills_outcome`). A failed or empty plan is the tool's error, and the builder goes on.
4. **When:** the builder's prompt says to call it during Blueprint whenever the app uses
   `claude:sample` or is a step in an AI pipeline, before writing AI code, and again when the AI's
   job changes; to put "How the AI works" in the README; and checklist item 11 checks the result.
5. **Both hosts:** Wardian's Make an app runs it in its loop, on the builder's meter and daily cap.
   The hosted builder gets `skillsRequest` and `skillsOutcome` from `wasm/workshop` and runs the
   request as its own step, paid for like the others (its ADR in wardian-web).

## Consequences

- An app's AI gets a designed skill and checked tools, and the same tested loop in every app.
- The skill and tools are never in the parts people see, and only one frame can spend tokens.
- An app with AI costs one more request to build, of up to 8,000 tokens out.
- The loop is JSON over `claude:sample`, not native tool use: one model reply per step, so a long
  plan takes as many requests as steps. Native tool use would need a new format (SPEC 9).
- The skills agent sees only what the package says about itself. A blueprint written first gives it
  more to work from, which the prompt asks for.

## Implementation

`static/ui/agent.js`; `domain::components` (the `agent` entry, `AI_APP`, `wire_suite`);
`usecases::workshop` (`SKILLS_TOOL`, the prompt, `skills_request`, `skills_outcome`);
`usecases::studio` (`Api::skills`, `Api::send`); `wasm/workshop` (`skillsRequest`, `skillsOutcome`);
`docs/site/ai.md` "Skills and tools", `docs/site/components.md`, the app-factory skill.
Tests: `tests/ui-agent.js`, `workshop_skills_agent_…`, `studio_dispatches_the_skills_agent_…`.

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- workshop_skills studio_dispatches && node tests/ui-agent.js`
