# OPDS catalog

| URL | Content |
|-----|---------|
| `/` | Catalog root: recently added, by title, folders, authors, series and search |
| `/opds/books?sort=name-asc` | Every book. `sort` is `date-desc` (default), `date-asc`, `name-asc` or `name-desc` |
| `/opds/folders/<path>` | Subfolders and books of a folder |
| `/opds/authors` | Authors, each linking to their books |
| `/opds/series` | Series, each linking to its books in volume order |
| `/opds/search?q=<terms>` | Search by title, author, series, path or subject |

Feeds are paginated with `?page=N`. The old `/?sort=...` URLs redirect to `/opds/books`.

When authentication is enabled, readers log in with HTTP Basic auth using the same username and password.

## Other endpoints

| URL | Content |
|-----|---------|
| `/admin` | Web panel |
| `/read/<path>` | Read a book in the browser |
| `/download/<path>` | Download the original file |
| `/healthz` | Health check (used by the Docker image) |
