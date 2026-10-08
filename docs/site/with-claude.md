# Make apps with Claude

Press **Make an app** and say in your own words what the app should do. Claude writes the app,
Wardian checks it, your browser tries it, and the app appears in your list. To change an app later,
open it and press **Change this app**.

## Set up a provider

Wardian reaches Claude through one of two providers. Set it up once in
**Settings → Make apps with Claude**.

### The Anthropic API

1. Make an API key in the Anthropic Console.
2. Paste it in Settings, and press **Save key**.

Wardian tests the key, then keeps it on the server, readable by its owner only. The key never goes
back to the browser. If the key is not scoped to one workspace, also give the workspace ID
(Console → Settings → Workspaces). Each app you make uses some API credit on that key.

### Amazon Bedrock

If you reach Claude through AWS, choose **Amazon Bedrock** in the same card (ADR-2610071106).

1. Give the region.
2. Give a Bedrock API key, or AWS access keys (with a session token for temporary credentials).
3. Press **Test and use Bedrock**.

Wardian tests them with one small request before it saves them to `data/bedrock.json`. The models
must be enabled for your account in that region (Bedrock console → Model access). By default Wardian
uses the US inference profiles of Claude Sonnet 4.5 (to build apps) and Claude Haiku 4.5 (for quick
requests). Set `WARDIAN_BEDROCK_MODEL` and `WARDIAN_BEDROCK_QUICK_MODEL` for another geography
(`eu.`, `apac.`, `global.`) or a newer model. AWS profiles, SSO and instance roles are not read.

## Describe the app well

Claude builds what you describe. A good description says:

- **what goes in**: "I paste a list of names, one per line";
- **what comes out**: "a random pairing, with nobody paired twice in a row";
- **what to keep**: "remember last week's pairs";
- **what it must never do**: "never show more than ten pairs at once".

You do not need to choose the kind. Claude chooses a page for one job and a suite for several,
following the same rules as this documentation.

## What happens while it builds

You do not have to wait on the chat. Claude works on the server, so you can open other apps, or close
the tab. The **Make an app** button shows how it is going: *working*, *testing*, *ready* or
*needs you*. Every Wardian tab in this browser finds the same chat.

1. Claude works on a copy of the app through a few tools: list, read, write and delete files, and
   check. It cannot touch anything outside that one app.
2. When Claude finishes, Wardian runs `wardian check`. A package with errors is not saved, and
   Claude gets the errors to fix.
3. Wardian saves the app as a new version in its **History**.
4. Your browser opens the app in a hidden frame and collects its errors. It sends them to Claude to
   fix, at most twice for each of your messages.

Claude cannot compile anything here, so the apps it makes are plain JavaScript, HTML and CSS: a page
app (Wardian adds the empty `app.wasm` that marks the folder as an app) or a suite. An app made by
Claude runs in the same sandbox as any other app.

Chats live in the server's memory. A restart ends them, but the apps they saved stay.

## History

Every save is a version. **History**, next to **Change this app**, lists every version with
Claude's reason for it. It compares any version with the app as it is now, and puts one back.
Putting a version back is a new version too, so nothing is lost. Before the first change to an app,
its original is kept as version 1. Each app keeps its last 50 versions.

## Ship an app Claude made

Apps Claude makes live in the working folder, `DATA_DIR/apps`, not in your repository. To commit
one, copy it back:

```
wardian promote my-app
git add apps/my-app
```

## Claude inside your app

This page is about Claude building apps. An app can also ask Claude questions while it runs, with
the `claude:sample` capability. See [Claude inside your app](/docs/ai).
