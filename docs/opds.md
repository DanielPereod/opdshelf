# OPDS catalog

| URL | Content |
|-----|---------|
| `/` | Catalog root: continue reading, recently added, by title, folders, authors, series, random and search |
| `/opds/reading` | Books started and not finished, last read first |
| `/opds/random` | A page of books picked at random, different on every visit |
| `/opds/books?sort=name-asc` | Every book. `sort` is `date-desc` (default), `date-asc`, `name-asc` or `name-desc` |
| `/opds/folders/<path>` | Subfolders and books of a folder |
| `/opds/authors` | Authors, each linking to their books |
| `/opds/series` | Series, each linking to its books in volume order |
| `/opds/search?q=<terms>` | Search by title, author, series, path or subject |

Feeds are paginated with `?page=N`. The old `/?sort=...` URLs redirect to `/opds/books`.

Feed titles are in English or Spanish, following the reader's `Accept-Language`.

When authentication is enabled, readers log in with HTTP Basic auth using the same username and password.

## Facets

Book lists (all books, folders, an author, a series, search and random) offer OPDS facets, which readers that support them show as filters. They can also be added to the URL:

| Parameter | Values |
|-----------|--------|
| `status` | `unread` (not started), `reading` or `finished`, as the web panel shows them, marks included |
| `lang` | A language code from the books' metadata, such as `es` |
| `format` | A format, such as `epub` or `cbz` |

Language and format facets are listed only when the books have more than one.

## Comics page by page (OPDS-PSE)

CBZ, CBR and CB7 entries carry an [OPDS-PSE](https://github.com/anansi-project/opds-pse) stream link, so readers such as Chunky, Panels or KOReader can show a comic page by page without downloading the whole file. Pages are served by `/page/<path>?n=N`, counted from 0.

## Other endpoints

| URL | Content |
|-----|---------|
| `/admin` | Web panel |
| `/read/<path>` | Read a book in the browser |
| `/download/<path>` | Download the original file |
| `/healthz` | Health check (used by the Docker image) |
