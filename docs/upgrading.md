# Upgrading from 1.x

Version 2 is a rewrite in Rust of the original Bun server. The Docker image name, port (`3000`) and books volume (`/app/books`) are unchanged, so pulling the new image is usually enough. Things to know:

- **The catalog root is now a navigation feed.** `/` lists "Recently added", "All books by title", folders, authors and search instead of every book. Readers that pinned `/` still work but show these entries first; point them to `/opds/books` for the old flat list. `/?sort=...` URLs redirect there.
- **`BASE_URL` replaces `REVERSE_PROXY` / `REVERSE_PROXY_HOST`.** The old pair is still read and treated as `BASE_URL`, but new setups should use `BASE_URL`.
- **Everyone has to log in again.** Session cookies are now signed, so cookies from 1.x are rejected.
- **A `.opdshelf` folder appears inside the books folder** to hold edited metadata, custom covers and thumbnails. It is hidden from the library. Set `DATA_DIR` to store it elsewhere.
- **Only book formats are listed.** Other files in the folder (audio, images, `.opf`, etc.) no longer show up. Adjust with `ALLOWED_EXTENSIONS` / `EXCLUDE_EXTENSIONS`.
- Running from source now needs Rust instead of Bun (see [From source](../README.md#from-source)).
