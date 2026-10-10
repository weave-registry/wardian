# Install and run

Wardian is one program. It holds its web pages, the component library, the templates and these
docs inside itself, so it needs no other files to run.

## Install with one command

On macOS or Linux, with no Rust and no `sudo`:

```
curl -fsSL https://github.com/weave-registry/wardian/releases/latest/download/install.sh | sh
wardian start --at-login
```

The script downloads the build for your system, checks it against `SHA256SUMS`, and installs
`wardian` and the example apps into `~/.local` (ADR-2610080915). `WARDIAN_VERSION`,
`WARDIAN_PREFIX` and `WARDIAN_DOWNLOAD` choose the version, the folder and where to download from.
If `~/.local/bin` is not on your `PATH`, the script prints the line to add. `wardian start
--at-login` runs Wardian in the background, starts it again each time you log in, and opens the app
list in your browser. Run the install line again to update: it replaces the program and restarts a
Wardian that runs in the background, so the new version takes over.

## Run in the background or in a terminal

Wardian runs in one of two ways:

| Command | Runs | Stops |
|---|---|---|
| `wardian start` | In the background, as a service of your system: launchd on macOS, systemd on Linux. It restarts after a crash. Your prompt comes back at once. | `wardian stop` |
| `wardian start --at-login` | The same, and it starts each time you log in. | `wardian stop` (also ends the start at login) |
| `wardian` | In this terminal, with a short summary. | Ctrl-C |

`wardian status` says whether Wardian runs, how, at which address, which version, which apps folder,
and whether it starts at login. It exits `0` when Wardian runs and `3` when it does not. A Wardian in
the background writes its output to `wardian.log` in the data folder (ADR-2610081800).

On Linux without systemd, such as in some containers, `wardian start` runs Wardian as a plain
background process and says that it will not restart after a crash or start at login.

`wardian start` and `wardian` both open the Wardian that already runs, if it is the same version
serving the same apps folder. If another Wardian holds the port, an older one or one serving
another folder, they leave it running, take the next free port up to 8010, and name the other one
(ADR-2610080930).

## Build from source

You need Rust (from [rustup.rs](https://rustup.rs)) and a copy of the repository.

```
git clone https://github.com/weave-registry/wardian.git
cd wardian
cargo install --path .
wardian
```

Open <http://127.0.0.1:8000>. The app list shows the example apps.

To change the WebAssembly in an app or a template, also add the WebAssembly target once:

```
rustup target add wasm32-unknown-unknown
```

## Run from a checkout

While you work on Wardian itself, you can run it straight from the repository:

```
cargo run --release                 # serves DATA_DIR/apps on http://127.0.0.1:8000
cargo run --release -- /path/to/apps
```

Do not keep a long-running Wardian on `target/release/wardian`. Each `cargo build` replaces that
file, and macOS can stop a program whose file was replaced while it runs. Install a copy with
`cargo install --path .`, or copy the file somewhere else first. On macOS, remove the old copy
before you copy a new one over it:

```
rm -f bin/wardian && cp target/release/wardian bin/
```

A file rewritten in place keeps its old signature record, and macOS stops it when it starts.

## Packages for macOS and Linux

The release scripts build ready-to-run packages into `dist/`:

```
scripts/package-macos.sh     # dist/Wardian.app and Wardian-<version>-macos-<arch>.zip
scripts/package-linux.sh     # dist/wardian-<version>-linux-<arch>.tar.gz
```

The macOS app opens `.wardian` files from Finder. On Linux, `install.sh` in the tarball registers
the same file type and a desktop entry. [Contributing](/docs/contributing#releasing) explains
signing and notarizing.


## Docker

```
ADMIN_TOKEN=<long random string> docker compose up -d --build
```

Docker listens on `0.0.0.0`, and its requests do not come from the same machine. So Docker needs
`ADMIN_TOKEN`. [Run Wardian for others](/docs/operate) explains why.

## The working folder

Wardian serves and saves apps in its **working folder**, `DATA_DIR/apps`. Without `DATA_DIR`, the
data folder is `./data` in a Wardian checkout or where `./data` already exists, and otherwise your
user data folder: `~/Library/Application Support/Wardian` on macOS, `~/.local/share/wardian` on Linux
(ADR-2610080915). Every Wardian has the example apps built in, however it was installed or started.
On each start it adds every example app the working folder has not had before: from `./apps` in a
checkout, else the copy installed beside the program, else the built-in copies. An example you remove
stays removed. An example you never changed is updated to the new version's, with the one before kept
in its History; one you changed is kept (ADR-2610081600, ADR-2610101000). It leaves out build
output (`target/`, `node_modules/`, `Cargo.lock`).

After that, apps you make or change inside Wardian change only the working folder, never the
repository. To ship one of them, copy it back and commit it:

```
wardian promote splunk-table   # copies DATA_DIR/apps/splunk-table into ./apps
git diff -- apps/splunk-table
```

If you name a folder (`wardian /path/to/apps`), Wardian serves that folder as it is. When that
folder is inside a git repository, Wardian says so at start, because changes you make in the app
then show up in git.

## The first start

When the data folder is empty, Wardian shows a short setup once. It asks where your apps come
from, offers to connect a Claude provider, and offers to set an admin token. You can skip every
step and change it later in **Settings**.

## Back up the master key

Wardian seals the keys you save in Settings under a master key it makes on the first start, in
`~/.config/wardian/master.key`. Keep a copy somewhere other than the data folder:

```
wardian key export ~/somewhere-safe/wardian.key
```

[Keys and Claude settings](/docs/keys#sealed-at-rest) explains how to restore it.

## Next

- [Your first ten minutes](/docs/tour): open, arrange, share and remove an app.
- [Settings and environment](/docs/config): every environment variable and file in the data folder.
