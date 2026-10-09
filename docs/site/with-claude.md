# Make apps with Claude

Press **Make an app** and say in your own words what the app should do. Claude writes the app,
Wardian checks it, your browser tries it, and the app appears in your list. To change an app later,
open it and press **Change this app**.

## Set up a provider

Wardian reaches Claude through one of two providers. Set it up once in
**Settings → Claude**.

### The Anthropic API

1. Make an API key in the Anthropic Console.
2. Paste it in Settings, and press **Save key**.

Wardian tests the key, then keeps it on the server, readable by its owner only. The key never goes
back to the browser. If the key is not scoped to one workspace, also give the workspace ID
(Console → Settings → Workspaces). Each app you make uses some API credit on that key.

### Amazon Bedrock

If you reach Claude through AWS, choose **Amazon Bedrock** in the same card. Wardian signs in to Bedrock in one of three ways (ADR-2610071106, ADR-2610091530). Choose the
first that fits:

1. **An AWS profile.** The profiles in `~/.aws/config` and `~/.aws/credentials` are listed with
   their regions; choose one, or **Other…** to type a name. The region fills in from the profile.
   Wardian keeps the profile's name and region, never its keys: it asks AWS CLI v2
   (`aws configure export-credentials --profile <name> --format process`) for short-lived keys
   when it needs them, and keeps them in memory until five minutes before they expire. That
   covers every kind of profile: access keys, IAM Identity Center (SSO), assumed roles, MFA and
   `credential_process`. Without the AWS CLI, Wardian reads a profile's access keys itself, or
   runs its `credential_process`; an SSO or role profile then needs AWS CLI v2. Wardian records
   where the AWS CLI is when you save, so a Wardian started with `wardian start`, which has a bare
   `PATH`, finds it. When an SSO sign-in expires, run `aws sso login --profile <name>`, then try
   again.
2. **A Bedrock API key**, made in the Bedrock console.
3. **AWS access keys**, with a session token for temporary keys. They expire; a profile does not
   need them typed again.

Press **Test and use Bedrock**. Wardian signs one tiny request before it saves the settings, sealed,
in `data/bedrock.json`. The models must be enabled for your account in that region (Bedrock console
→ Model access). By default Wardian
uses the US inference profiles of Claude Sonnet 4.5 (to build apps) and Claude Haiku 4.5 (for quick
requests). Set `WARDIAN_BEDROCK_MODEL` and `WARDIAN_BEDROCK_QUICK_MODEL` for another geography
(`eu.`, `apac.`, `global.`) or a newer model. Instance roles and container credentials are not read.

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
