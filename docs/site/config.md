# Settings and environment

Wardian has two places for settings. **Environment variables** are read once, at start. **Settings**
in the browser change things while Wardian runs, and Wardian saves them in its **data folder**.
Where both can set the same thing, the table below says which one wins.

## The data folder

The data folder holds everything Wardian keeps: your apps, their data, keys and permission answers.

Wardian uses `DATA_DIR` if it is set. Otherwise it uses `./data`, under the folder you start Wardian
in. A relative `DATA_DIR` is also read from that folder. At start Wardian prints the full path
(`data: …`) and stops if it cannot write there.

| How you run Wardian | Data folder |
|---|---|
| `wardian` or `cargo run` | `./data` under the current folder, or `DATA_DIR` |
| The macOS app (`Wardian.app`) | `~/Library/Application Support/Wardian` (its launcher sets `DATA_DIR`) |
| The Linux desktop entry | `$XDG_DATA_HOME/wardian`, by default `~/.local/share/wardian` (its launcher sets `DATA_DIR`) |
| Docker (`docker compose`) | `/data`, a volume |

The plain `wardian` command picks its folder itself (ADR-2610080915, `src/config.rs`,
`choose_data_dir`): `DATA_DIR` when set; `./data` in a Wardian checkout, or when `./data` already
exists; otherwise the platform's folder, `~/Library/Application Support/Wardian` on macOS and
`$XDG_DATA_HOME/wardian` (by default `~/.local/share/wardian`) on Linux.

Git ignores `/data` in the repository.

## Files in the data folder

Files marked *private* are written readable by their owner only (mode `600`). Files marked
*sealed* are also encrypted under a master key kept outside the data folder
([Keys and Claude settings](/docs/keys#sealed-at-rest)).

| File or folder | What it holds |
|---|---|
| `apps/` | The working folder: the apps Wardian serves and saves. Every example app is added once ([Install and run](/docs/install#the-working-folder)). |
| `apps/.examples-seen` | The example apps the working folder has been given, so one you remove stays removed. |
| `apps/.trash/` | Removed apps, until you empty it yourself. Hidden folders are never listed or served. |
| `config.json` | The chosen app source: local, or a Google Drive folder and its name. *Private.* |
| `service-account.json` | The uploaded Google service account key. *Private, sealed.* |
| `anthropic-key` | The Anthropic API key. *Private, sealed.* |
| `anthropic-workspace` | The Anthropic workspace ID, if you gave one. *Private.* |
| `bedrock.json` | The Amazon Bedrock region and keys, or the AWS profile's name, its region and the AWS CLI's path (never a profile's keys). *Private, sealed.* |
| `ai-provider` | Which Claude provider Settings chose: `anthropic` or `bedrock`. *Private.* |
| `agent.json` | Claude's models, the limits of **Make an app** and `claude:sample`, and the daily caps ([Keys and Claude settings](/docs/keys)). *Private.* |
| `usage.json` | The tokens Claude used, by UTC day and by app, for the last 31 days. *Private.* |
| `key-checks.json` | The last test of each key: when, and what the service said. No key. *Private.* |
| `admin-token` | The admin token saved in **Settings → Keys**. *Private, sealed.* |
| `splunk.json` | The Splunk address and account. *Private, sealed.* |
| `grants.json` | Your answers to permission questions: channels, Splunk, Claude, reading another app's tables. *Private.* |
| `state/` | Arrange layouts (`layouts.json`), each app's saved data (`apps/<app>.json`) and the latest message on each channel (`channels.json`). *Private.* |
| `db/<app>.sqlite` | Each app's own database, for the `db` capability. *Private.* |
| `history/<app>/` | Each app's versions, and `log.json` saying when, by what and why. The last 50 versions are kept. |
| `wardian.log` | Every start and stop of the server, with the time in UTC. Past 1 MB it moves to `wardian.log.1`. |
| `first-run` | Present until the first-run setup is finished or skipped. |

Wardian also writes and removes `.write-test` at start, to prove it can write the folder.

To start over, stop Wardian and move the data folder away. The next start is a first start: it shows
the setup again and fills `apps/` from `./apps`.

## Environment variables

| Variable | Default | Meaning |
|---|---|---|
| `ADDR` | `127.0.0.1:8000` | The address to listen on. Anything but `127.0.0.0/8`, `::1` or `localhost` needs `ADMIN_TOKEN`. Port `0` lets the system pick a free port. |
| `DATA_DIR` | `data` | The data folder. |
| `ADMIN_TOKEN` | none | Whoever sends it is an admin, and nobody else is. It wins over a token saved in **Settings → Keys**. A non-loopback address needs one of the two. |
| `REFRESH_SECS` | `60` | How often to read the Drive folder again. Values below `5` count as `5`. |
| `GDRIVE_FOLDER_ID` | none | Start on this Drive folder. It wins over the folder saved in Settings. |
| `GDRIVE_SA_KEY` | none | Path of a service account key file. Used only if no key was uploaded in Settings. |
| `IMPORT_ALLOW_LAN` | off | `1` lets an import link point at the local network. Loopback and cloud metadata addresses stay blocked. |
| `ANTHROPIC_API_KEY` | none | Anthropic API key. Used only if none is saved in Settings. |
| `ANTHROPIC_WORKSPACE_ID` | none | Anthropic workspace ID, for a key that is not scoped to one workspace. Used only if none is saved. |
| `WARDIAN_AI_MODEL` | `claude-opus-5-5` | The Anthropic model that writes apps. `RUSTLE_AI_MODEL` is read if this is not set. A model chosen in **Settings → Claude** wins. |
| `WARDIAN_AI_PROVIDER` | `anthropic` | `bedrock` to use Amazon Bedrock. Used only if Settings has not chosen a provider. |
| `AWS_REGION` | none | Bedrock region. `AWS_DEFAULT_REGION` is read if this is not set. |
| `AWS_BEARER_TOKEN_BEDROCK` | none | Bedrock API key. |
| `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` | none | AWS access keys for Bedrock, instead of an API key. The session token is for temporary keys. |
| `AWS_PROFILE` | none | An AWS profile for Bedrock, when neither an API key nor access keys are set. Its region is used unless `AWS_REGION` is set ([Claude inside your app](/docs/ai#amazon-bedrock)). |
| `AWS_CONFIG_FILE` | `~/.aws/config` | The AWS config file, where Settings → Claude finds profiles. `wardian start` passes it to the service. |
| `AWS_SHARED_CREDENTIALS_FILE` | `~/.aws/credentials` | The AWS credentials file, read with the config file. `wardian start` passes it to the service. |
| `WARDIAN_AWS_CLI` | the first `aws` on `PATH`, then in `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` | The AWS CLI that signs in to a profile. `none` reads profiles without it. |
| `WARDIAN_BEDROCK_MODEL` | `us.anthropic.claude-sonnet-4-5-20250929-v1:0` | The Bedrock model that writes apps. A model chosen in **Settings → Claude** wins. |
| `WARDIAN_BEDROCK_QUICK_MODEL` | `us.anthropic.claude-haiku-4-5-20251001-v1:0` | The Bedrock model for quick requests. A model chosen in **Settings → Claude** wins. |
| `SPLUNK_URL` | none | Splunk management address. Used only if none is saved in Settings. |
| `SPLUNK_TOKEN` | none | Splunk token. Or set `SPLUNK_USERNAME` and `SPLUNK_PASSWORD`. |
| `SPLUNK_INSECURE_TLS` | off | `1` or `true` accepts any certificate, such as Splunk's self-signed default. |
| `SPLUNK_CA_FILE` | none | A PEM file of extra certificate authorities to trust for Splunk. |
| `WARDIAN_SPLUNK_API_PORT` | `8089` | The port **Settings → Splunk** tries for Splunk's API after the address given (ADR-2610091500). |
| `WARDIAN_MASTER_KEY` | none | The master key that seals saved keys, as 64 hex digits. When set, no other place is used ([Keys and Claude settings](/docs/keys#sealed-at-rest)). |
| `WARDIAN_MASTER_KEY_FILE` | `~/.config/wardian/master.key` | The file for the master key, outside the data folder. Made on the first start. |
| `WARDIAN_SERVICE_LABEL` | `studio.wardian` | The service `wardian start`, `stop` and `status` manage ([Command line](/docs/cli#start-stop-status)). Must start with `studio.wardian`; tests use one of their own. |
| `GDRIVE_API_BASE` | Google | For tests only. |
| `ANTHROPIC_BASE_URL` | Anthropic | For tests only. |
| `WARDIAN_BEDROCK_BASE_URL` | the region's endpoint | For tests only. |

An empty variable counts as not set.

Bedrock from the environment needs a region and either `AWS_BEARER_TOKEN_BEDROCK` or both access key
variables, or `AWS_PROFILE` (the region may then come from the profile). Wardian does not read
instance roles or container credentials.

The test scripts read more variables, such as `WARDIAN_BROWSER`. [Contributing](/docs/contributing)
lists them.

### Which one wins

| Setting | Saved in Settings | Environment |
|---|---|---|
| App source | used | `GDRIVE_FOLDER_ID` wins |
| Drive key | wins | `GDRIVE_SA_KEY` used if none was uploaded |
| Anthropic key and workspace | win | used if none is saved |
| Claude provider | wins | `WARDIAN_AI_PROVIDER` used if none was chosen |
| Claude models | win | the `WARDIAN_*_MODEL` variables are the defaults |
| Splunk account | wins | used if none is saved |
| Admin token | used if `ADMIN_TOKEN` is not set | `ADMIN_TOKEN` wins |

## Settings in the browser

Open **Settings** from the app list. It has nine sections.

| Section | What you can do | Needs admin |
|---|---|---|
| **Status** | See the app source, its folder, the number of apps and, for Drive, the last refresh and the last error. **Refresh now** reads the app source again. Enter the admin token here when Settings is locked. | to refresh |
| **Keys** | See every key and account, where each comes from and its last test. Test one again, or remove it. Set or make the admin token ([Keys and Claude settings](/docs/keys)). | yes |
| **App source** | Serve the local folder, or connect a Google Drive folder: upload a service account key, choose a folder, connect. | yes |
| **Claude** | Choose the Anthropic API or Amazon Bedrock, and give its keys. Wardian tests them before it saves them. Choose the models and the limits. | yes |
| **Usage** | See the tokens Claude used for each app and for **Make an app**, today and over 31 days, and set each one's daily cap. | yes |
| **Splunk** | Give the management address and a token, or a username and password. Allow a self-signed certificate. Wardian tests the account before it saves it. | yes |
| **Permissions** | See every answer to a permission question. Revoke one, and the app asks again next time. | to revoke |
| **Removed apps** | Restore an app you removed. | yes |
| **Import** | Import a `.wardian` or `.zip` file, from your computer or from a link. | yes |

Keys and passwords go to the server and never come back to the browser. Settings shows whether one
is saved, and an API key's last four characters at most.

### First start

When the data folder is empty, Wardian shows a short setup once. It asks where your apps come from,
offers to connect a Claude provider, and offers to set an admin token. Each step can be skipped and
changed later in Settings.

## Who is an admin

Only an admin can change Settings, import and export apps, browse Drive, see an app's history, and
use the server for Splunk searches, Claude requests and app databases. Wardian decides per request:

- **An admin token is set**, by `ADMIN_TOKEN` or in **Settings → Keys**. A request is from an admin
  only if it carries the token in the `X-Admin-Token` header. This is true for every address, this machine included. Settings asks for
  the token and keeps it in that browser tab only (`sessionStorage`), so a new tab asks again.
- **No admin token is set.** A request is from an admin only if it comes from this machine
  (a loopback address) and its `Host` header names `localhost`, `127.0.0.1` or `::1`. Then every
  program and browser on this machine is an admin. That is why Wardian refuses to listen on any other
  address without a token.

A viewer who is not an admin can still open and use apps. Their Arrange layouts and the apps' saved
data stay in their own browser instead of on the server. Capabilities that need the server
(`splunk`, `claude:sample`, `db`) do not work for them.

[Run Wardian for others](/docs/operate) shows how to set this up, and [Security model](/docs/security)
explains the reasons.
