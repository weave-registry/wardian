# ADR-2610131300: an AI app's skill and tools are in its settings, editable and improvable, and it has a chat

**Status:** Accepted
**Date:** 2026-10-13
**Drivers:** The user: "each app that is AI enabled should have tools/skill repository. this should
be available through the app specific settings and made editable, ai enhancable", then "every AI
enabled too should also have an AI chatbot interface that loads these tools for exploring its
data". Chosen with the user: settings edit the tools' data and Claude may propose new tools, which
Make an app builds; the chat is a drawer run by Wardian, not part of each app; Wardian first, then
the hosted builder.

## Context

- **Where the AI lives:** ADR-2610131000 put an app's AI in an out-of-sight suite app named `ai`,
  the only one with `claude:sample`, running `WardianUI.agent({sample, skill, tools})`. The skill
  and the tools were constants in that app's JavaScript, so nobody could see or change them without
  changing code.
- **A tool has two halves:** what the model is told (name, description, input schema) is data;
  `run` is code that reads the app's data. Only the first half can be edited safely outside Make an
  app, which checks and tests code.
- **No app settings yet:** an app's header has Change this app and a ⋯ menu (History, Download…).
  Every save of a local app goes through a check and into the app's History (ADR-2610071122).
- **The kernel** routes calls between a suite's frames by contract. Wardian's own page is not a
  frame, and could not call an app's method.

## Decision

1. **The repository is two files in the `ai` app's folder:** `skill.md` (the method the model
   follows) and `tools.json`, `{"format": 1, "tools": [{"name", "description", "input",
   "enabled", "proposed"}]}`. `domain::ai_tools` holds the rules, the same ones agent.js checks
   against: snake_case unique names, a description, an object input schema using only the keywords
   agent.js checks, at most 32 tools, a skill of at most 20,000 characters, and all of it under
   40,000 characters so a question still fits in one `claude:sample` request. `wardian check`
   applies them to any package that has the files, so no save path can keep a broken repository.
2. **The code holds only `run`.** `WardianUI.agentApp(ctx, {run, answer, db})` reads both files
   with `ctx.asset` and builds the loop from the tools that are on, not proposed, and have a `run`.
   `tools()` says which were left out and why. With `db: true` and the `db` capability,
   `list_tables` and `read_rows` are built in: they need no code, and they always use agent.js's
   own input schema (at most 50 rows), whatever the file says.
3. **App settings:** in a local suite with an `ai` app, ⋯ → **AI skill and tools** opens an editor
   for the skill and each tool: name, description, input schema, on or off, with a badge for a tool
   that is proposed, has no code, or is built in. `GET /api/apps/<app>/ai` reads; `POST` saves
   through the same checked save as Make an app (`save_package`), recorded in History as made by
   `app-settings`. Both need admin and the local source, as Change this app does.
4. **Improve with AI:** `POST /api/apps/<app>/ai/improve` sends Claude the editor's repository, the
   README and the `ai` app's code, with the user's wish, under its own prompt (`usecases::ai_tools`)
   that marks all but the wish as data. Claude returns the whole repository and notes. The reply is
   checked against the same rules, and any tool whose name is not a word in the code (and is not
   built in) comes back as `proposed`, whatever Claude said. Nothing is saved: the editor shows
   Claude's version with Undo, and the user saves it or not. It runs on Make an app's meter and cap.
   **Build proposed tools with Make an app** opens Make an app on the app with the request written.
5. **Ask:** an app whose `ai` app provides `chat` has **Ask** in its header. Ask is a panel run by
   Wardian's page. Each message goes to the kernel as `{wardian: "ai", k: "chat", messages}`, from
   Wardian's own page only (same origin, the parent window), and the kernel invokes the `ai` app's
   `chat` method as it would a call from another frame, waiting up to 15 seconds for it to start.
   `agentApp`'s `chat(messages)` answers the last user message from the last 20 messages with the
   same skill and tools, as `{reply, steps}`, and the panel shows the reply and the tools used.
   Ask spends the app's own `claude:sample` allowance, after the user's permission, like any other
   use of Claude by the app.
6. **The builder:** Make an app's prompt, the skills agent's prompt and the component guide say to
   put the skill and tools in `skill.md` and `tools.json`, load them with `agentApp`, and provide
   `chat`. Checklist item 11 checks it. The hosted builder gets this with the next vendored
   `workshop.wasm`, which also exposes the repository (`aiTools`, `aiToolsApply`) and Improve with
   AI (`aiImproveRequest`, `aiImproveOutcome`), so the hosted builder applies the same rules and
   prompt (its ADR in wardian-web).

## Consequences

- An app's AI can be read and tuned by its user without touching code, and every change can be
  undone from History.
- Claude can suggest tools but never adds code from settings. A suggested tool does nothing until
  Make an app builds it, and the AI leaves it out until then.
- The same skill and tools serve the app's own jobs and the chat. A skill written only for one job
  may need a line for open questions; Improve with AI can add it.
- Apps built before this keep their constants and work as before. Settings show them with an empty
  repository and offer Make an app to move to `agentApp` and add `chat`.
- Ask only reads what the tools read. A tool that changes data would also run from the chat, so
  the rule of ADR-2610131000 stands: such a tool returns a proposal the user confirms.

## Implementation

`domain::ai_tools` (the rules, `builtin_tools`, `ai_app`); `domain::check` (the files' check);
`usecases::ai_tools` (the Improve prompt, `has_code`, `improve_outcome`); `usecases::studio`
(`ai_tools`, `save_ai_tools`, `improve_ai_tools`, `save_package` shared with Make an app); `usecases::ai_tools` (`view`, `apply`, `improve_request_for`, `improve_outcome_for`, shared with `wasm/workshop`);
`ports::service::Builder`; `adapters::primary::http` (the three routes); `static/ui/agent.js`
(`agentApp`); `static/kernel.html` (the chat bridge); `static/index.html` (the menu item, the
editor, Ask); `usecases::workshop` and `domain::components` (the prompts and the guide);
`docs/site/ai.md`, `docs/site/components.md`.
Tests: `domain::ai_tools::tests`, `usecases::ai_tools::tests`,
`app_settings_save_the_ai_skill_and_tools_as_a_version`, `improve_with_ai_returns_a_proposal_and_saves_nothing`,
`tests/ui-agent.js` (agentApp), `tests/run-ai-tools-e2e.sh` (Ask, the editor, Improve, Save, History).

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- ai_tools app_settings improve_with_ai && node tests/ui-agent.js && tests/run-ai-tools-e2e.sh`
