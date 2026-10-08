# Your first ten minutes

This page walks you through Wardian as a user. You open an app, change its layout, save a copy,
send it to someone and remove it. You need Wardian running ([Install and run](/docs/install)).

## 1. Open an app

1. Open <http://127.0.0.1:8000>.
2. Click **loan-planner** in the app list.
3. Change the amount or the rate. The summary, the chart and the export update at once.

What you see is a **suite**: six small apps on one screen. The inputs on the left, the summary,
the chart and the export on the right. Each one runs in its own sealed frame. They never touch each
other. They send messages through the kernel, which checks each message against the suite's
contract. [How Wardian works](/docs/concepts) explains this.

Now open **adder**. It has no page at all. Wardian read the functions the WebAssembly file exports
and drew one card per function, with an input per number. That is a **module** app.

## 2. Arrange it your way

1. Press **Arrange** at the bottom left of the app.
2. Drag a panel to a new place, or use its buttons to move it up, down or to the other column.
3. Hide a panel you do not need. Choose one column if your screen is narrow.
4. Press **Done**.

Wardian keeps your layout in its data folder. A restart, another browser or cleared site data does
not lose it. The app itself never changes. **Reset** brings back the app's own layout.

## 3. Save what you see

Press **Save as web page**, then **Save**. Wardian writes one `.html` file of the app as you see it
now: your layout, your inputs, the results and each chart as a picture. It opens in any browser,
offline, with no Wardian. It is a copy that does not update. Check it before you send it, because
everything on screen goes into the file.

## 4. Send the app to someone

Press **Download**. Wardian saves the app as a `.wardian` file. Anyone with Wardian can import it
in **Settings → Import a Wardian file**. Tick **Include my data** to add what the app has saved; Wardian
lists what goes in first. Keys, accounts, permission answers and history never go in.

When someone imports the file, Wardian first shows what it holds and what it may use. Permissions
are always asked again. [Share, import and remove apps](/docs/sharing) has the details.

## 5. Make one of your own

Press **Make an app** and describe a small tool in a sentence or two:

> A tip calculator. I type the bill and the number of people, and pick 15, 18 or 20 percent.
> Show each person's share, rounded up to the next coin.

Claude writes the app, Wardian checks it, and your browser tries it in a hidden frame. Errors go
back to Claude to fix. The app appears in your list. You need a Claude provider for this; see
[Make apps with Claude](/docs/with-claude). To write one yourself, follow
[Building Wardian apps](/docs/guide).

## 6. Change it, then go back

Open your new app and press **Change this app**. Ask for a change. Wardian keeps each version in the
app's **History**: every version, Claude's reason for it, a comparison with the app as it is now, and
a button to put it back. Putting a version back is a new version too, so nothing is lost.

## 7. Remove it

Press **Remove app**, then confirm. The app moves to `.trash/` in the working folder. Nothing is
deleted. Press **Undo** at once, or restore it later in **Settings → Removed apps**.

## What next

- See what others have built: [Example apps](/docs/examples).
- Learn the model behind the screen: [How Wardian works](/docs/concepts).
