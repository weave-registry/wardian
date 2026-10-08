# Keys and Claude settings

Wardian holds up to five secrets: the Anthropic API key, the Amazon Bedrock keys, the Splunk
account, the Google service account key and the admin token. **Settings → Keys** lists them all.
**Settings → Claude** chooses the models and the limits, and **Settings → Usage** shows how much
Claude was used and stops an app at its daily cap (ADR-2610081500).

Every page here is for admins only.

## The key list

Each secret shows:

- **Where it comes from.** *Saved in Settings*, *from the environment* (such as
  `ANTHROPIC_API_KEY`), or *not set*. A secret saved in Settings wins over the environment's.
- **What it is**, never the secret itself: the last four characters of an API key, the Bedrock
  region, the Splunk address and user, the service account's address.
- **Its last test**: when, and whether the service accepted it. Wardian tests a secret before it
  saves it, when you press **Test again**, and records it when Claude refuses a key while an app
  uses it.

| Button | What it does |
|---|---|
| **Test again** | Tests the saved secret with the service now, and records the answer. |
| **Change** / **Set up** | Opens the section where the secret is typed: Claude, Splunk or App source. |
| **Remove** | Removes the secret saved in Settings. Click twice. The environment's value, if any, then applies. |

Removing the Google service account key while Google Drive is the app source switches the source
to the local apps folder.

A secret that is saved but cannot be read shows *cannot be read*, with the reason. Wardian then acts
as if it were not set: type it again, or remove it.

## Sealed at rest

Wardian seals every saved secret with AES-256-GCM, under a master key it keeps outside the data
folder (ADR-2610081501). A copy of the data folder alone holds no readable key. The top of the key
list says where the master key is.

| Where the master key is | When |
|---|---|
| `WARDIAN_MASTER_KEY` | set to 64 hex digits |
| the file `WARDIAN_MASTER_KEY_FILE` names | set; the file is made on the first start |
| `~/.config/wardian/master.key` | otherwise (`$XDG_CONFIG_HOME` moves it); made on the first start |

Wardian does not use the operating system's credential store. Back up the key file apart from the
data folder: either one alone holds no readable key.

A secret saved by an older Wardian is sealed the first time this one reads it, at start. A data
folder moved to another machine, without its master key, keeps its apps and data, but each secret
shows *cannot be read* until it is typed again. Wardian never makes a new master key while a sealed
secret is in the data folder, so a lost key is reported instead of hidden.

If the key file cannot be made, Wardian keeps secrets as plain files that only their owner can
read, as before, and says so at start and on the key list. Set `WARDIAN_MASTER_KEY_FILE` to a file
outside the data folder to fix it.

## The admin token

With an admin token, every change to Settings needs it, from every address, this machine included.
Without one, every program and browser on this machine is an admin, and nobody else is
([Security model](/docs/security#who-is-an-admin)).

- **Make one for me** makes a token of 64 random hex digits and shows it once. Copy it.
- **Save this token** saves one you type: 24 to 200 characters, no spaces.
- The browser tab that set it keeps it until the tab is closed. Another browser types it in
  **Settings → Status → Unlock**.
- `ADMIN_TOKEN` wins. While it is set, Settings shows the token as set by the environment, and
  cannot change it.
- A saved token counts at start: Wardian may then listen on an address other machines can reach.
  While it does, the saved token cannot be removed, only replaced.

If you lose the token, stop Wardian, remove `admin-token` from the data folder, and start it again.

## Models and limits

**Settings → Claude → Models and limits** sets, for each provider, the main model and the quick
model (`modelTier: 'quick'`). Leave a field empty to use Wardian's default, which the field shows
in grey. Wardian tests a new model with the provider's saved keys before it saves it, so a typo
never replaces a working model. A provider with no keys cannot test it: the model is saved, and the
answer says it was not tested.

| Setting | Default | Allowed | What it limits |
|---|---|---|---|
| Steps in one turn | 40 | 5 to 200 | model requests **Make an app** makes for one message |
| Tokens in one reply (Make an app) | 16,000 | 1,000 to 64,000 | the length of each of its replies |
| Chats kept at once | 20 | 1 to 100 | **Make an app** chats kept in memory |
| Tokens per day (Make an app) | 0, no cap | 0 to 100,000,000 | all of **Make an app**'s requests in a UTC day |
| Tokens in one reply (apps) | 4,000 | 256 to 16,000 | each `claude:sample` answer |
| Tokens per app per day | 200,000 | 0 to 100,000,000 | each app's `claude:sample` requests in a UTC day |

The settings are kept in `agent.json` in the data folder. They win over `WARDIAN_AI_MODEL`,
`WARDIAN_BEDROCK_MODEL` and `WARDIAN_BEDROCK_QUICK_MODEL`, which stay the defaults.
**Back to the defaults** clears every model and limit.

## Usage and caps

**Settings → Usage** lists each app that used Claude, and **Make an app**: the tokens used today,
over the last 31 days, the number of requests, and the daily cap. Wardian counts every token Claude
read, cached or not, and every token it wrote, as the API reports them. It counts tokens, not
money: prices differ between the Anthropic API and Amazon Bedrock.

Change an app's cap in its row. `0` means no cap. An app's own cap replaces **Tokens per app per
day** for that app.

When an app reaches its cap, its next `claude:sample` request fails with `e.code === 'over_budget'`
until the next UTC day, and no request is sent ([Claude inside your app](/docs/ai#handle-every-error)).
**Make an app** at its cap stops the turn with a message. A request already under way finishes, so
a day can end a little over its cap.

The counts are kept in `usage.json` in the data folder.

## The files

| File | What it holds | Holds a secret |
|---|---|---|
| `anthropic-key`, `bedrock.json`, `splunk.json`, `service-account.json`, `admin-token` | the secrets, sealed | yes |
| `agent.json` | models, limits and caps | no |
| `usage.json` | tokens by day and app, for 31 days | no |
| `key-checks.json` | the last test of each secret | no |

All are readable by their owner only. [Settings and environment](/docs/config#files-in-the-data-folder)
lists every file in the data folder.
