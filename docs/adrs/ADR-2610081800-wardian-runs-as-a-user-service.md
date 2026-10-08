# ADR-2610081800: Wardian runs as a user service

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user asked why Wardian "doesn't go into the background", then for the best way to do
it, and chose `start`, `stop` and `status` with an `--at-login` option. The `wardian` command a curl
install gives runs in the foreground until Ctrl-C; only the packaged desktop launchers start it out
of sight.

## Context

A program that detaches itself needs a file holding its process id, which goes stale after a crash
or a reboot, and nothing restarts it when it fails. macOS (launchd, LaunchAgents) and most Linux
desktops and servers (systemd user units) already start, watch, restart and stop a user's programs,
and start them at login. Neither needs `sudo` for a user's own service.

## Decision

1. **Three commands.** `wardian start [--at-login]`, `wardian stop`, `wardian status`. Plain
   `wardian` stays the foreground server.
2. **The system runs it.** On macOS, `start` writes `~/Library/LaunchAgents/studio.wardian.plist`
   (the program's full path, `DATA_DIR` resolved by ADR-2610080915's rule and written in full,
   `WARDIAN_NO_OPEN=1`, output appended to `<data>/wardian.log`, `KeepAlive` on a crash only) and
   loads it with `launchctl bootstrap gui/<uid>`; on Linux with a systemd user session,
   `~/.config/systemd/user/wardian.service` (`Restart=on-failure`) and `systemctl --user start`.
   Without either, `start` runs Wardian detached with its output in the log, keeps its process id
   in `<data>/wardian.pid`, and says that it will not restart on a crash or start at login.
3. **`--at-login`.** macOS: `RunAtLoad` true; Linux: `systemctl --user enable`. Without it the
   service runs now only. `start` without the flag on a service set to start at login keeps that.
4. **`start` opens the app.** It waits until `/api/status` answers (up to 15 s), then opens the
   browser as ADR-2610080930 does, and prints a short block: address, apps folder, whether it starts
   at login, and `wardian stop`. A Wardian already running, the same one, is opened, not started
   again; another Wardian on the port is named, as ADR-2610080930 does.
5. **`stop`** unloads the service (macOS `launchctl bootout`, Linux `systemctl --user stop`, and
   `disable` too), or ends the process in `wardian.pid`, and removes nothing else. **`status`**
   says whether it runs, how (service or plain), its address, version, apps folder, and whether
   it starts at login; it exits 0 when running and 3 when not.
6. **Updates.** The installer, when it replaces `bin/wardian` and a Wardian service exists, restarts
   it so the new version runs, and says so.

## Consequences

- Wardian runs without a terminal open, comes back after a crash, and can start at login.
- The service file names the program's full path; moving the program needs `wardian start` again,
  which rewrites the file.
- The service manager is outside Wardian's control in tests: the commands it runs go through one
  port, faked in tests.

## Implementation

- A driven port for the service manager (`ports/service_manager.rs` or similar), adapters for
  launchd, systemd and the plain fallback, the unit and plist text as pure functions with tests.
- `cli.rs`: the three commands; `main.rs` wires them. `scripts/install.sh`: restart on update.
- README (Run), CHANGELOG, `wardian --help`.
- Found in implementing it: launchd starts any job with `KeepAlive` when it loads it, whatever
  `RunAtLoad` says (`man launchd.plist`), and it loads every file in `~/Library/LaunchAgents` at
  login. So the plist is kept there only with `--at-login`; a service that runs now only keeps it
  in `~/.config/wardian/<label>.plist` and is bootstrapped from there. `stop` removes the file (and
  the systemd unit), so a stopped service stays stopped after the next login, and the installer
  restarts only a service whose file is there. `WARDIAN_SERVICE_LABEL` (default `studio.wardian`)
  lets a test use a label of its own.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release service_`

## References

- ADR-2610080915 (install with one command), ADR-2610080930 (a friendly first run)
