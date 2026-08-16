# Bitcoin Universe fork provenance

This repository is the Bitcoin Universe production fork of
`Trac-Systems/ord-dogecoin`.

- Upstream baseline: `1ae4f3fe832493042612034130af2ea68d9f39ac`
- Upstream release: `1.0.2`
- Production integration branch: `develop`
- Production release branch: `main`

The fork keeps upstream history and an `upstream` remote. Bitcoin Universe
changes must pass the pinned Linux, macOS, and Windows CI before promotion to
`main`.

The first production patch corrects new-index feature persistence so
`--index-drc20` controls the DRC20 index independently of `--index-dunes`.
Indexes created by the affected upstream build without DRC20 data must be
rebuilt; the flag cannot add missing historical protocol state to an existing
database.
