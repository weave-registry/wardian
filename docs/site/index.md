# Wardian documentation

Wardian keeps the small tools you make, often with AI, on a computer you control. It serves them
to your browser from a local folder or from Google Drive. Each app runs **sealed** in its own
browser frame: it cannot see Wardian's settings, your files or your other apps unless you allow it,
and the browser blocks its requests to anywhere but its own package
([three narrow routes stay open](/docs/security#three-routes-stay-open), and a test keeps that list true).

Think of a Wardian case, the sealed glass box that carried living plants across oceans in the
1800s. Your apps are the plants. Wardian is the case. Each plant gets its own pane of glass, and
the doors between them, and out to the world, open only where the case allows them.

## Where to start

| You want to… | Read |
|---|---|
| install Wardian and open your first app | [Install and run](/docs/install), then [Your first ten minutes](/docs/tour) |
| understand what a package, a suite and a kernel are | [How Wardian works](/docs/concepts) |
| build an app yourself | [Building Wardian apps](/docs/guide) |
| have Claude build an app for you | [Make apps with Claude](/docs/with-claude) |
| see what is possible | [Example apps](/docs/examples): sixteen working apps, from a unit converter to a risk simulator |
| look up an exact rule | [Package format](/docs/spec), [The ctx API](/docs/ctx), [Capabilities](/docs/capabilities) |
| run Wardian for a team | [Run Wardian for others](/docs/operate) and [Security model](/docs/security) |

## Three kinds of app

Every Wardian app is one folder, called a **package**. It comes in one of three kinds. Start with
the simplest kind that fits, and move up when you need to.

| Kind | What it is | Good for | Example |
|---|---|---|---|
| **module** | A WebAssembly file and nothing else. Wardian draws the interface: one input per parameter and a Run button. | Calculators, converters, any function of numbers | [`unit-converter`](/docs/examples#unit-converter) |
| **page** | A module (or plain JavaScript) with your own HTML page, in a sandboxed frame. | One tool with one job and a real interface | [`image-lab`](/docs/examples#image-lab) |
| **suite** | Several small apps on one screen. Each runs in its own sealed frame and talks to the others only through the **kernel**. | Anything with parts: inputs, a computation, several views, an export | [`loan-planner`](/docs/examples#loan-planner) |

## What an app may use

A suite app starts with nothing: no network, no files, no storage. It asks for each thing it needs by
name, and Wardian grants only what the app declares. Some things also need your consent the first
time.

| Capability | Gives the app | You are asked first? |
|---|---|---|
| `storage` | a small key–value store, kept by Wardian | no |
| `asset` | read files from its own package, such as a `.wasm` | no |
| `worker` | a Web Worker, for heavy work off the screen | no |
| `claude:downloads` | save a file to your computer | no |
| `db` | its own SQLite database, for large tables | no; reading another app's tables: yes |
| `claude:sample` | ask Claude a question, through the provider you set up | yes |
| `splunk` | run a Splunk search with the server's account | yes |
| channels | send or receive messages with other packages | yes |

The full list, with every method, is in [Capabilities](/docs/capabilities).

## Why it is built this way

Most small tools die in one of two places: a folder of HTML files nobody can find, or a hosted
platform that wants an account, a subscription and your data. Wardian sits between the two. Your
apps live on your disk, in plain folders you can read, change, copy and commit. And because each
app is sealed, you can run one you did not write, including one an AI wrote a minute ago, without
handing it the keys to everything else.
