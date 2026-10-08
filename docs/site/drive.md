# Serve apps from Google Drive

Wardian can read its apps from a Google Drive folder instead of a local folder. You keep the apps in
Drive, share them the way you share any Drive folder, and every Wardian that reads the folder shows
the same apps.

Wardian only reads Drive. It asks Google for read-only access, and it never writes, moves or
deletes a Drive file.

## Connect

1. In the Google Cloud console, create a project and turn on the Drive API.
2. Create a service account. Under **Keys**, add a JSON key and download it.
3. In Wardian, open **Settings → App source → Google Drive**. Upload the key file. Wardian tests the
   key with Google before it saves it.
4. Copy the service account's address that the page shows. In Drive, share your apps folder with
   that address as **Viewer**.
5. Back in Settings, under **2. Choose the apps folder**, open the folder in the browser, or paste
   its link and press **Use link**. The page lists the apps it found.
6. Under **3. Connect**, press **Connect to this folder**. Wardian switches with no restart and
   remembers the folder.

Put one app folder per app inside the Drive folder, exactly as in a local folder.

## How it stays up to date

Wardian reads the folder again every `REFRESH_SECS` seconds (default 60). **Refresh now** in
Settings reads it at once. Wardian keeps each file by its Drive checksum, so it downloads an
unchanged app once and fetches a changed one again.

## Settings from the environment

| Variable | Meaning |
|---|---|
| `GDRIVE_FOLDER_ID` | start on this folder; wins over the saved one |
| `GDRIVE_SA_KEY` | key file path, used only if no key was uploaded |
| `REFRESH_SECS` | how often to read the folder again |

The uploaded key is kept as `DATA_DIR/service-account.json`, readable by its owner only.

## What changes with Drive

- **Make an app** and **Change this app** save to the working folder, not to Drive.
- **Remove app** cannot remove a Drive app. Remove it in Drive.
- Importing a zip from a Drive file link uses the same service account. Share the file with it.
