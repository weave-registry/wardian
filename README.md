# wasm-host

Serves small WebAssembly apps to a browser, from a local folder or straight from Google Drive.

## Layout (local or Drive)

    <root>/
      adder/
        app.wasm
      other-app/
        app.wasm

## Local mode

    cargo run --release            # serves ./apps on 127.0.0.1:8000
    cargo run --release -- /path/to/apps

## Google Drive mode (no rclone)

1. Google Cloud console: create a project, enable the Drive API, create a service account,
   download its JSON key as `service-account.json`.
2. In Drive, share your apps folder with the service account's `client_email` as **Viewer**.
3. Copy the folder ID from its URL (`.../folders/<ID>`).
4. Run:

       GDRIVE_FOLDER_ID=<ID> GDRIVE_SA_KEY=./service-account.json cargo run --release

   or with Docker: `GDRIVE_FOLDER_ID=<ID> docker compose up -d --build`

Env vars: `GDRIVE_FOLDER_ID`, `GDRIVE_SA_KEY`, `REFRESH_SECS` (default 60),
`ADDR` (default 127.0.0.1:8000), `GDRIVE_API_BASE` (testing only).

The server only ever requests the read-only Drive scope, indexes the folder in the background,
and caches each file by Drive md5, so an unchanged app is downloaded once and a changed one is re-fetched.
