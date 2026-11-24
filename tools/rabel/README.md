# RABEL catalogue POC

This crate is an intentionally small connectivity probe for the Salamanca Public
Library (Casa de las Conchas) catalogue.

It submits an all-fields search, follows AbsysNet's HTML meta-refresh into the
session-specific results URL, and can fetch an individual bibliographic record.
It saves raw HTML for inspection. It does **not** yet parse bibliographic records,
inspect holdings, normalize availability, register an agent tool, or integrate
with `agent-server`.

## Run against the live catalogue

From the repository root:

```sh
cargo run -p rabel-tool --bin rabel-poc -- "Don Quijote"
```

This saves the search page to `rabel-search.html`. To also fetch a record (the
document number is the `DOC` value in a result link):

```sh
cargo run -p rabel-tool --bin rabel-poc -- "Don Quijote" rabel-search.html 1
```

The record page is saved as `rabel-record-1.html`. The optional second argument
selects the search output path; the optional third argument selects the record
number to fetch.

## Session handling

The initial search request is:

- `SUBC=SAL/BPSA` — Biblioteca Pública del Estado en Salamanca
- `ACC=DOSEARCH`
- `xsqf01=<query>`

RABEL responds with a small HTML page containing a `meta http-equiv="Refresh"`
whose URL includes a session token. This is not an HTTP redirect, so reqwest
does not follow it automatically. The client extracts that URL, resolves it
against the bootstrap URL, and GETs it using the same reqwest client and cookie
store. The session URL is then used as the base for record links:

```
GET <session-url>/NT1?ACC=165&DOC=<document-number>
```

## What is verified and what is not

Verified manually against the live catalogue:

- The OPAC entry point is `https://rabel.jcyl.es/cgi-bin/abnetopac2`.
- `SUBC=SAL/BPSA` selects the Salamanca public library catalogue.
- The initial GET accepts `ACC=DOSEARCH` and `xsqf01` and starts a search.
- The meta-refresh creates a session-specific URL; requesting it returns the
  actual result list (the sample query "Don Quijote" returned 302 records).
- Result links use `ACC=165&DOC=<n>` for individual records.

The client currently captures the record-detail HTML but does not interpret it.
The next experiment is to inspect that page for item/holding information
(library, shelf mark, copy status, and availability) and determine whether
additional requests are needed.

## References

- <https://directoriobibliotecas.mcu.es/dimbe.cmd?apartado=informespdf&accion=exportar_detalle&ps=5081&filtro=>
- <https://www.comunidadbaratz.com/blog/como-lanzar-consultas-bibliograficas-a-absysnet-traves-de-la-url-del-opac/>
- <https://www.comunidadbaratz.com/blog/como-crear-urls-estables-al-opac-de-absysnet-y-no-morir-en-el-intento/>
