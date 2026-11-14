# Reading progress sync

The web reader saves where you are on the server, so a book opens at the same place in any browser. While offline it keeps reading from a local copy and sends the position when the connection is back. Positions are stored in `DATA_DIR/progress.json`.

A page number means a different place on every screen size, so EPUB positions point to a place in the text instead. To pin the exact sentence you stopped at, select it in the EPUB reader and choose **Set as last position**; other devices open the book on that sentence and highlight it.

Which position wins:
- The most recent one, by when it was read (not when it reached the server), so a position saved offline earlier does not replace newer reading done elsewhere.
- A device that sends again the position it already had (readers do this when they close or sleep) has not moved, so it does not undo reading done meanwhile on another device.
- The last 30 positions of each book are kept.

## Highlights and notes

Highlights and notes sync between the web reader, the Android app and KOReader, in both directions:

- **Web reader**: select text to pick a colour (it highlights at once), add a **Note**, copy it or make it the last position. Click a highlight to change its note or colour, or delete it. The highlighter button in the bar opens the book's highlights beside the page.
- **Android app**: select text and choose **Highlight** or **Note**; tap a highlight to edit it. The contents sheet has a **Highlights** tab, and the book's page lists them too.
- **KOReader**: highlight as usual; the plugin sends new, changed and deleted highlights a couple of seconds later and draws the ones made elsewhere. They show in KOReader's own bookmarks list (**Tools → OPDShelf → Highlights and notes**). **Sync highlights and notes** in the same menu turns it off.
- **The book's page** in the web panel has a **Highlights** tab: grouped by chapter in reading order, with search, filters by colour and "with a note", editing, a link that opens the reader right at each highlight, and export to Markdown or JSON.

Each highlight has an id made where it was created, so every device edits the same one; the most recent change of each highlight wins and deletions reach devices that were offline. While a book is open, changes arrive live in the web reader and KOReader (the Android app checks every 30 seconds). Highlights are kept in `DATA_DIR/annotations.json`.

A highlight is stored with KOReader's xpointers of its start and end, the text, the words around it and, from the web, the EPUB CFI. When a device cannot follow the xpointers (another copy of the book, or a reader that splits the text differently) it finds the text instead. Highlights in PDFs and comics sync and are listed everywhere, but only drawn in the reader they were made in.

## KOReader

Install the OPDShelf plugin from [`koreader/opdshelf.koplugin`](../koreader/opdshelf.koplugin): copy the `opdshelf.koplugin` folder into KOReader's `plugins` folder (`.adds/koreader/plugins` on Kobo, `koreader/plugins` on Kindle and in Android's storage) and restart KOReader. Then in **Tools → OPDShelf → Server** enter the server address and OPDShelf's username and password. If Progress sync already pointed to `…/kosync`, the address and username are filled in.

**Live sync.** The plugin syncs with its own protocol, not KOReader's Progress sync (which it offers to turn off):

- The position is sent a few seconds after each page turn, when the book is closed and when the device goes to sleep. Without a connection it is kept and sent, with the time it was read, once back online.
- While a book is open the plugin keeps a connection to the server (`/api/progress/events`), so reading done on the phone or in the browser reaches the Kobo at once, and the other way round.
- Positions sent while offline carry the device's clock at sending time, so the server can correct e-readers whose clock is wrong before comparing them.

**Conflicts.** When the server has a position from another device:

| Situation | What happens |
|-----------|--------------|
| The other device is further ahead | KOReader goes there and says from where (**When another device is further ahead** can ask instead, or stay) |
| The other device is behind | It asks before going back |
| You are turning pages right now | It never moves the page under you: it asks |
| Both devices read since they last synced (for example one offline) | It shows both positions and lets you choose; the one you keep becomes the latest everywhere |
| A sentence was set as last position on another device | It offers to go there and highlights it |

**Set as last position**: select some text and choose it in the selection menu. Other devices open the book on that sentence. **Go to where another device left it** lists the latest position of each device, to jump back to any of them.

**Library.** **Tools → OPDShelf → Library** (also a gesture action) browses the server's catalog with covers, as a list or a grid, by recent books, folders, authors and series, with search. Tap a book for its cover, series, subjects and description, and to download it or download and read it; a long press downloads it straight away. Downloaded books are marked, with how far they were read. Books go to the download folder chosen in the menu. Downloads run in the background, so the screen keeps refreshing on e-ink devices such as Kindle and a tap on the message cancels them.

**Home screen.** Once installed, OPDShelf works as KOReader's launcher: its home screen opens when KOReader starts and after closing a book, and the file browser's home button goes back to it. At the top there is a status bar with the time, date, Wi-Fi and battery. Below it you find the book to continue, with its cover, progress and, when another device read further, where that one is; then the newest books on the server; then the last books read on the device, with their progress. A tab bar at the bottom leads to Home, Library, Search, Files and Settings. Pull down to refresh. Without a connection it shows what it last saw from the server. Turn it off with **Tools → OPDShelf → Show the home screen at start**, or choose another screen in **File browser → Settings → Start with**. It can also be opened from **Tools → OPDShelf → Home** or with a gesture action.

The plugin's texts follow KOReader's language; it includes Spanish.

Books are matched by KOReader's partial MD5 of the file, so a position follows a book that is renamed or moved. KOReader's EPUB positions (xpointers) and the web reader's are translated in the browser, with the surrounding text saved alongside to find the place again if the two disagree.

KOReader's own **Progress sync** also works without the plugin: set its custom server to `https://your-server/kosync` and log in with OPDShelf's username and password (keep **Document matching method** on **Binary**). It only syncs when opening and closing books, and sends the top of the page.

## Progress API

| Method | URL | Content |
|--------|-----|---------|
| `GET` | `/api/progress/<document>` | `current` position and `history` of a book |
| `PUT` | `/api/progress/<document>` | Save a position: `progress`, `percentage` (0–1), `device`, `device_id`, and optionally `cfi`, `context` (`before`, `after`, `selected`), `manual`, `timestamp` (Unix seconds, when it was read) and `sent_at` (the device's clock when sending, to correct a wrong clock). Answers the `current` position, whether yours was `accepted` (false when a newer one from another device won) and `server_time` |
| `GET` | `/api/progress` | The `current` position of every book that has one, newest first, with the `path` and `title` of the book (`null` when the file is not in the library) |
| `GET` | `/api/progress/events` | Server-sent events: `hello` (`server_time`), then `progress` (`document`, `current`) on every change of a book's current position, `annotations` (`document`, `annotations`) with the highlights that changed, and `resync` if some were missed. `?documents=<id>,<id>` limits it to those books. Adding `&device_id=<id>&device=<name>` shows the device as connected in the web panel while the stream is open |
| `GET` | `/api/devices` | Every device that saved a position or is listening: `device_id`, `device`, `last_seen`, `live`, `connections`, number of `books` and `positions`, and its `recent` books with their percentage |
| `DELETE` | `/api/devices/<device_id>` | Forget a device: removes its positions. Needs the panel's `X-CSRF-Token` header |
| `GET` | `/api/annotations/<document>` | Every highlight of a book in reading order: `id`, `text`, `note`, `color` (`yellow`, `orange`, `red`, `purple`, `blue`, `cyan`, `green`, `olive`, `gray`), `style` (`lighten`, `underscore`, `strikeout`, `invert`), `pos0`, `pos1`, `cfi`, `context`, `chapter`, `page`, `percentage`, `created`, `updated`, `device`, `device_id`. Deleted ones come as `{id, deleted: true, updated}` |
| `PUT` | `/api/annotations/<document>` | Send new, changed or deleted highlights: `annotations`, `device`, `device_id`, `sent_at`. The newest change of each one wins. Answers every highlight of the book |
| `GET` | `/api/annotations` | Books with highlights: `highlights`, `notes`, last `updated`, `path` and `title` |
| `GET` | `/api/annotations/<document>/export` | The highlights as a file: Markdown by chapter, or JSON with `?format=json` |
| `GET` | `/api/highlights` | Every highlight of the library, newest first, each with its `document`, `path`, `title` and `authors` |
| `GET` | `/api/highlights/review?date=YYYY-MM-DD` | The daily review: 5 highlights at random, from different books when possible, the same all day on every device |
| `GET` | `/api/highlights/export` | Every book's highlights in one file: Markdown, or JSON with `?format=json` |
| `*` | `/kosync/...` | KOReader sync protocol (`users/auth`, `syncs/progress`) |

`<document>` is KOReader's partial MD5 of the file. `progress` is an xpointer for reflowable books and the page number (from 1) for PDFs and comics.
