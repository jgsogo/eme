# RABEL catalogue POC

This crate is an intentionally small connectivity probe for the Salamanca Public
Library (Casa de las Conchas) catalogue.

It currently does only one thing: submit an all-fields search to the OPAC and
save the raw HTML response. It does **not** parse bibliographic records, inspect
holdings, normalize availability, register an agent tool, or integrate with
`agent-server`.

## Run against the live catalogue

From the repository root:

```sh
cargo run -p rabel-tool --bin rabel-poc -- "Don Quijote"
```

An optional second argument selects the output file:

```sh
cargo run -p rabel-tool --bin rabel-poc -- "Miguel de Unamuno" /tmp/unamuno.html
```

The command prints the HTTP status, content type, final URL (including redirects),
and response size. The HTML is saved locally so that the actual markup can be
inspected and converted into regression fixtures.

## What is verified and what is not

The configured entry point and subcatalogue are taken from the Ministry of
Culture's library directory:

- OPAC: <https://rabel.jcyl.es/cgi-bin/abnetopac2>
- Subcatalogue: `SUBC=SAL/BPSA` (Biblioteca Pública del Estado en Salamanca)

The request uses `ACC=DOSEARCH` and `xsqf01` for an all-fields query. These
parameter names follow the standard AbsysNet URL-search convention documented
by Baratz, but **have not yet been verified against this specific RABEL
installation**. This POC is intended to test that assumption. It also enables
a cookie store and follows redirects so that session behaviour is visible in
the final URL and returned HTML.

References:

- <https://directoriobibliotecas.mcu.es/dimbe.cmd?apartado=informespdf&accion=exportar_detalle&ps=5081&filtro=>
- <https://www.comunidadbaratz.com/blog/como-lanzar-consultas-bibliograficas-a-absysnet-traves-de-la-url-del-opac/>
- <https://www.comunidadbaratz.com/blog/como-crear-urls-estables-al-opac-de-absysnet-y-no-morir-en-el-intento/>

## Next step

Run the probe and inspect the saved HTML. Once the endpoint, query parameters,
redirect/session behaviour, and result markup are confirmed, add focused
fixtures and implement the bibliographic parser. Holdings and availability
should remain a separate follow-up step.
