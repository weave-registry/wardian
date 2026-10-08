# Share, import and remove apps

An app is a folder, so you can always share it as a folder. Wardian adds three easier ways: a
`.wardian` file that another Wardian imports, a web page that opens anywhere, and an import from a
link.

## Send an app as a `.wardian` file

1. Open the app.
2. Press **Download**.
3. To add what the app has saved, tick **Include my data**. Wardian lists what goes in first.
4. Save the file.

The file holds the app exactly as `wardian check` reads it. With your data, it also holds the app's
`storage`, its Arrange layout and its `db` tables. It never holds keys, accounts, permission
answers or history. Anyone you send it to can read everything in it, so read the list.

From the command line:

```
wardian export my-app                       # my-app.wardian
wardian export my-app shared/my-app.wardian --with-data
```

A `.wardian` file is a zip with one package folder at its top. Its manifest is in
`<package>/.wardian/export.json` ([Package format §7.2](/docs/spec#7-distribution)).

## Import a `.wardian` file or a zip

1. Open **Settings → Import a Wardian file**.
2. Press **Choose a file**, or paste a link and press **Import link**.
3. Read what Wardian shows: the app, what it may use, and any data.
4. Tick **Also install its data** if you want its data.
5. Press **Import**.

Permissions are always asked again, even if the sender allowed them. Data the import replaces is
kept in the app's history folder. Wardian refuses an app whose name is taken, unless you tick
**Replace apps that already exist**.

### Project zips

The import keeps each app's folder tree, so a whole project zip works. For each `.wasm` file, the
app's top folder is:

1. the nearest folder above it with an `app.json`, or
2. the folder it sits in, skipping build folders (`pkg/`, `dist/`, `build/`, `out/` …).

So `usl-wasm/pkg/usl_wasm.wasm` gives the app `usl-wasm`, with `demo/`, `pkg/` and the rest kept in
place. A folder that holds `suite.json` is a package top, with everything inside it. A folder with
several `.wasm` files and no `app.wasm` is skipped, because the import cannot tell which one is the
app. Do not depend on these rules for your own packages; use the layout in
[Package format §7.2](/docs/spec#7-distribution).

### Limits

| Limit | Value |
|---|---|
| A zip, uploaded or by link | 100 MB |
| A zip from Google Drive | 64 MB |
| One file | 64 MB |
| All files, unpacked | 256 MB |
| Entries in a zip | 10,000 |

The importer unpacks into a hidden staging folder first, so a failed import changes nothing.

### Links

Links must be public internet addresses. Wardian refuses links to itself, to cloud metadata
(`169.254.169.254`) and to other internal addresses, also after a redirect. To import from a file
server on your own network, set `IMPORT_ALLOW_LAN=1`. A Google Drive file link works too: Wardian
fetches it with the service account, so share the file with that account.

## Save an app as a web page

1. Open the app and arrange it the way you want it seen.
2. Press **Save as web page**, then **Save**.

Wardian saves `<app>-<date>.html`: one file that opens in any browser, offline, with no Wardian. It
holds every panel you have not hidden, in your layout, with what you typed, the results and each
chart as a picture.

- It is a copy that does not update. Sorting, paging and buttons do nothing in it.
- It holds no script and makes no request, so it is safe to open.
- Everything on screen goes into the file. Check it before you send it.
- An app may put more in the file than it shows, such as every row of a table.
- A file over 25 MB is refused, with a message that names the largest part.

## Remove an app

1. Open the app.
2. Press **Remove app**, then confirm.

The app moves to `.trash/` in the working folder. Nothing is deleted. Press **Undo** at once, or
restore it later in **Settings → Removed apps**. To delete removed apps for good, empty `.trash/`
yourself.

Apps served from Google Drive are removed in Drive, not by Wardian: Wardian only reads Drive.

## Ship an app in the repository

Apps you make or change in Wardian live in the working folder. To commit one:

```
wardian promote my-app        # copies DATA_DIR/apps/my-app into ./apps
git add apps/my-app
```
