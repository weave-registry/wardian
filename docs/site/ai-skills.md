# Build with an AI assistant

Wardian ships two skills for AI coding assistants such as Claude Code. A skill is a folder of
instructions and tools that the assistant reads when a task matches. With them, you can say "make
me an app that tracks my reading list" in any folder, and the assistant builds a Wardian package,
checks it and tries it in a browser, by the same rules as these docs.

## Install them

```
cd my-wardian-apps
wardian skills
```

Wardian writes the skills into `./.claude/skills/`. Claude Code finds them there the next time it
starts in that folder.

| Skill | The assistant uses it when you… |
|---|---|
| `wardian-app-factory` | ask for a new app, a change to an app, or to turn a project into a package |
| `wardian-app-doctor` | say an app is broken, blank, stuck, shows a fault, or fails its check |

Each skill carries its own `references/`: the package format and the docs pages it needs, copied
from the Wardian that installed it. So the skills work offline, in any folder, and always match your
version of Wardian.

## Keep them current

After you update Wardian, install the skills again:

```
wardian skills --force
```

Without `--force`, Wardian keeps any skill file you changed and says which. With `--force`, it
replaces them all. `wardian skills --list` names the skills.

## What the factory does

1. It finds Wardian (`$WARDIAN_BIN`, then `target/release/wardian` in a checkout, then `wardian` on
   `PATH`) and the folder your Wardian serves.
2. It chooses the kind: a module for numbers, a page for one job, a suite for several. It tells
   you which and why.
3. It starts from `wardian new`, adds components with `wardian add`, and builds the app.
4. It runs `wardian check` and fixes every error.
5. It runs a smoke test: a private Wardian on a free port with a throwaway data folder, the app
   opened the way you would open it, and every fault reported.
6. It tells you what it built, how to open it, and what it could not verify.

The smoke test needs Node with the `playwright` package, and Google Chrome or Playwright's
Chromium. Without them, the skill says the browser test was skipped instead of claiming success.

## What the doctor does

It collects evidence in a fixed order (the check, the smoke test, the kernel's faults and trace,
the browser console, the server log), stops at the first thing that is wrong, matches it to a known
cause, fixes it, and proves the fix with the check and the smoke test again.

## Other assistants

The skills are Markdown with a short front matter, plus one Node script. An assistant that does not
read `.claude/skills/` can still use them: point it at `SKILL.md` and its `references/`.

## Make apps inside Wardian instead

You do not need an assistant of your own. **Make an app** in Wardian builds apps with Claude on the
server, by the same rules. See [Make apps with Claude](/docs/with-claude).
