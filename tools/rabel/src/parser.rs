//! Narrow parser for the HTML structures returned by the RABEL AbsysNet OPAC.
//!
//! The OPAC is not a documented API. Keep selectors and heuristics here and
//! cover them with saved response fixtures so markup changes fail visibly.

use regex::Regex;
use url::Url;

use crate::{client::RabelError, types::{Availability, CatalogueItem, CatalogueSearchResult, CopyHolding, Library}};

pub(super) fn meta_refresh_target(html: &str) -> Option<String> {
    let meta = Regex::new(r#"(?is)<meta\b[^>]*http-equiv\s*=\s*["']?refresh["']?[^>]*>"#).ok()?;
    let tag = meta.find(html)?.as_str();
    let content = Regex::new(r#"(?is)content\s*=\s*["']([^"']+)["']"#).ok()?
        .captures(tag)?.get(1)?.as_str();
    let url = Regex::new(r#"(?i)url\s*=\s*["']?([^"'\s;]+)"#).ok()?
        .captures(content)?.get(1)?.as_str();
    Some(url.to_owned())
}

pub(super) fn session_action(html: &str, base_url: &Url) -> Option<Url> {
    let forms = Regex::new(r#"(?is)<form\b([^>]*)>"#).ok()?;
    let attr = Regex::new(r#"(?is)\baction\s*=\s*["']([^"']+)["']"#).ok()?;
    forms.captures_iter(html).find_map(|form| {
        let attrs = form.get(1)?.as_str();
        let action = attr.captures(attrs)?.get(1)?.as_str();
        base_url.join(action).ok()
    })
}

fn form_diagnostics(html: &str) -> Vec<String> {
    let Ok(forms) = Regex::new(r#"(?is)<form\b([^>]*)>(.*?)</form>"#) else { return Vec::new(); };
    let Ok(controls) = Regex::new(r#"(?is)<(?:input|select|textarea|button)\b[^>]*>"#) else { return Vec::new(); };
    let Ok(attribute) = Regex::new(r#"(?is)\b([a-z_:][-a-z0-9_:]*)\s*=\s*["']([^"']*)["']"#) else { return Vec::new(); };
    forms.captures_iter(html).take(12).map(|form| {
        let body = form.get(2).map(|m| m.as_str()).unwrap_or("");
        let fields = controls.find_iter(body).take(24).map(|matched| {
            let tag = matched.as_str();
            let attributes = attribute.captures_iter(tag).filter_map(|capture| Some((
                capture.get(1)?.as_str().to_ascii_lowercase(),
                capture.get(2)?.as_str().to_owned()
            ))).collect::<std::collections::HashMap<_, _>>();
            let kind = if tag.to_ascii_lowercase().starts_with("<input") {
                attributes.get("type").cloned().unwrap_or_else(|| "text".to_owned())
            } else {
                tag.trim_start_matches('<').split(|c: char| c.is_whitespace() || c == '>')
                    .next().unwrap_or("").to_owned()
            };
            let name = attributes.get("name").cloned().unwrap_or_default();
            let value = if kind.eq_ignore_ascii_case("password") { Some("[redacted]".to_owned()) }
                else { attributes.get("value").cloned() };
            format!("{}:{}{}", kind, name, value.map(|v| format!("={v:?}")).unwrap_or_default())
        }).collect::<Vec<_>>();
        format!("form controls={fields:?}")
    }).collect()
}

pub(super) fn search_page_diagnostic(html: &str, response_url: &Url) -> String {
    let title = Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>").ok()
        .and_then(|re| re.captures(html)).and_then(|captures| captures.get(1))
        .map(|value| clean_text(value.as_str())).filter(|value| !value.is_empty())
        .unwrap_or_else(|| "(no title)".to_owned());
    let forms = form_diagnostics(html);
    let links = Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#).ok()
        .map(|re| re.captures_iter(html).filter_map(|captures| {
            let label = clean_text(captures.get(2)?.as_str());
            (!label.is_empty()).then(|| label.chars().take(100).collect::<String>())
        }).take(8).collect::<Vec<_>>()).unwrap_or_default();
    let body = Regex::new(r"(?is)<body\b[^>]*>(.*?)</body>").ok()
        .and_then(|re| re.captures(html)).and_then(|captures| captures.get(1))
        .map(|value| clean_text(value.as_str())).unwrap_or_else(|| clean_text(html));
    let excerpt: String = body.chars().take(320).collect();
    let _ = response_url;
    format!("page title={title:?}; forms={forms:?}; result links={links:?}; body excerpt={excerpt:?}")
}

pub(super) fn search_results(html: &str, session_url: &Url) -> Result<Vec<CatalogueSearchResult>, RabelError> {
    let link = Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#)
        .map_err(|error| RabelError::Parse(error.to_string()))?;
    let mut results = Vec::new();
    for captures in link.captures_iter(html) {
        let href = decode_entities(captures.get(1).unwrap().as_str());
        if !href.contains("ACC=165") || !href.contains("DOC=") { continue; }
        let record_url = session_url.join(&href)
            .map_err(|error| RabelError::Parse(format!("invalid record link: {error}")))?;
        let heading = clean_text(captures.get(2).unwrap().as_str());
        if heading.is_empty() { continue; }
        let row_html = surrounding_row(html, captures.get(0).unwrap().start());
        let context = clean_text(&row_html);
        let mut authors = extract_authors(&context);
        let (heading_title, responsibility) = split_statement_of_responsibility(&heading);
        let title = if authors.is_empty() {
            if let Some((author, title)) = split_dated_author_prefix(&heading_title) {
                authors.push(author);
                title
            } else {
                heading_title
            }
        } else {
            let stripped = strip_search_author_prefix(&heading_title, &authors);
            if stripped != heading_title {
                stripped
            } else {
                // Some RABEL rows expose the author field, but its formatting
                // does not exactly match the author prefix in the result link.
                // Still split the well-defined dated-author heading form.
                split_dated_author_prefix(&heading_title)
                    .map(|(_, title)| title)
                    .unwrap_or(heading_title)
            }
        };
        let publication_statement = labelled_value(&context, &[
            "Editor / Productor", "Publicación", "Publicacion", "Edición", "Edicion",
            "Datos de publicación", "Datos de publicacion",
        ]);
        let publication_year = publication_statement.as_deref().and_then(extract_publication_year);
        let publisher = publication_statement.as_deref().map(strip_publication_year).and_then(non_empty)
            .or_else(|| labelled_value(&context, &["Editorial"]));
        results.push(CatalogueSearchResult {
            record_ref: record_url.to_string(),
            title,
            authors,
            publication: publication_statement.clone(),
            publisher,
            publication_year,
            responsibility,
            publication_statement,
            physical_description: labelled_value(&context, &["Descripción física", "Descripcion fisica"]),
            series: labelled_value(&context, &["Colección editorial", "Coleccion editorial", "Serie"]),
            notes: labelled_value(&context, &["Nota de bibliografía", "Nota de bibliografia", "Notas"])
                .into_iter().collect(),
            subjects: labelled_value(&context, &["Materias", "Materia"]).into_iter().collect(),
            category: extract_document_category(&heading, &row_html),
            isbn: extract_isbn(&context),
        });
    }
    if results.is_empty() && !is_empty_search_page(html) {
        return Err(RabelError::Parse(format!("no recognized record links found in RABEL search response; {}", search_page_diagnostic(html, session_url))));
    }
    Ok(results)
}

/// If a search row omits its separate author field, recover the common
/// "Author (birth-death) Title" heading form without treating arbitrary
/// leading words as an author.
fn split_dated_author_prefix(value: &str) -> Option<(String, String)> {
    let re = Regex::new(r"^(.+?\(\d{3,4}(?:\s*[-–]\s*\d{3,4})?\))\s+(.+)$").ok()?;
    let captures = re.captures(value)?;
    Some((captures.get(1)?.as_str().trim().to_owned(), captures.get(2)?.as_str().trim().to_owned()))
}

/// Search headings commonly render as "Author Title / Responsibility".
/// The search row's author field lets us recover the title proper without
/// requesting the record page again.
fn strip_search_author_prefix(title: &str, authors: &[String]) -> String {
    for author in authors {
        if title.len() >= author.len()
            && title.get(..author.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(author))
        {
            let remainder = title[author.len()..].trim();
            if !remainder.is_empty() {
                return remainder.trim_start_matches([':', '-', '–']).trim().to_owned();
            }
        }
    }
    title.trim().to_owned()
}

fn is_empty_search_page(html: &str) -> bool {
    let text = clean_text(html).to_lowercase();
    ["no se han encontrado", "no se encontraron", "no hay resultados", "sin resultados",
     "no existen registros", "no hay registros", "0 resultados", "0 registros", "0 documentos"]
        .iter().any(|marker| text.contains(marker))
}

pub(super) fn record(html: &str) -> Result<CatalogueItem, RabelError> {
    let text = clean_text(html);
    if text.is_empty() { return Err(RabelError::Parse("empty record page".into())); }

    // RABEL places navigation before the bibliographic record. Its structured
    // #docu_body definition lists are the authoritative source.
    let fields = bibliographic_fields(html);
    let title_statement = field_value(&fields, &["Título", "Titulo"]);
    let (title, responsibility) = title_statement.map(|value| split_statement_of_responsibility(&value))
        .map(|(title, responsibility)| (non_empty(title), responsibility))
        .unwrap_or_else(|| (None, None));
    let authors = field_values(&fields, &["Autor / Autora", "Autor", "Autores", "Autoría", "Autoria"]);
    let publication_statement = field_value(&fields, &["Editor / Productor", "Editor", "Productor", "Publicación", "Publicacion"]);
    let publication_year = publication_statement.as_deref().and_then(extract_publication_year)
        .or_else(|| field_value(&fields, &["Fecha de publicación", "Fecha de publicacion", "Año de publicación", "Ano de publicacion", "Fecha de edición", "Fecha de edicion"]).as_deref().and_then(extract_publication_year));
    let publisher = publication_statement.as_deref().map(strip_publication_year).and_then(non_empty)
        .or_else(|| field_value(&fields, &["Editorial", "Editor"]));
    let physical_description = field_value(&fields, &["Descripción física", "Descripcion fisica"]);
    let series = field_value(&fields, &["Colección editorial", "Coleccion editorial", "Serie"]);
    let notes = field_values(&fields, &["Nota de bibliografía", "Nota de bibliografia", "Notas"]);
    let subjects = field_values(&fields, &["Materias", "Materia"]);
    let isbn = field_value(&fields, &["ISBN"]).and_then(|value| extract_isbn(&value))
        .or_else(|| extract_isbn(&text));

    // Fallback for older/alternate templates. Never infer a publication year
    // from the entire page: author birth/death dates are not edition dates.
    let title = title.or_else(|| labelled_value(&text, &["Título", "Titulo"]));
    let authors = if authors.is_empty() { extract_authors(&text) } else { authors };
    let publisher = publisher.or_else(|| labelled_value(&text, &["Editorial"]));
    let publication_year = publication_year.or_else(|| {
        labelled_value(&text, &["Editor / Productor", "Publicación", "Publicacion"])
            .as_deref().and_then(extract_publication_year)
    });

    Ok(CatalogueItem {
        title, authors, publisher, publication_year, isbn, responsibility,
        publication_statement, physical_description, series, notes, subjects,
    })
}

fn bibliographic_fields(html: &str) -> Vec<(String, Vec<String>)> {
    let body = Regex::new(r#"(?is)<(?:div|section)\b[^>]*id\s*=\s*["']docu_body["'][^>]*>(.*?)</(?:div|section)>"#).ok()
        .and_then(|re| re.captures(html)).and_then(|capture| capture.get(1)).map(|m| m.as_str().to_owned());
    let Some(body) = body else { return Vec::new(); };
    let Ok(definitions) = Regex::new(r"(?is)<dl\b[^>]*>(.*?)</dl>") else { return Vec::new(); };
    let Ok(label) = Regex::new(r"(?is)<dt\b[^>]*>(.*?)</dt>") else { return Vec::new(); };
    let Ok(value) = Regex::new(r"(?is)<dd\b[^>]*>(.*?)</dd>") else { return Vec::new(); };
    definitions.captures_iter(&body).filter_map(|definition| {
        let fragment = definition.get(1)?.as_str();
        let key = label.captures(fragment).and_then(|capture| capture.get(1))
            .map(|m| normalize_field_label(&clean_text(m.as_str())))?;
        let values = value.captures_iter(fragment).filter_map(|capture| {
            let value = clean_text(capture.get(1)?.as_str());
            (!value.is_empty()).then_some(value)
        }).collect::<Vec<_>>();
        (!values.is_empty()).then_some((key, values))
    }).collect()
}

fn normalize_field_label(value: &str) -> String {
    value.trim().trim_end_matches(':').split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn field_values(fields: &[(String, Vec<String>)], labels: &[&str]) -> Vec<String> {
    let wanted = labels.iter().map(|label| normalize_field_label(label)).collect::<Vec<_>>();
    fields.iter().filter(|(label, _)| wanted.contains(label))
        .flat_map(|(_, values)| values.iter().cloned()).collect()
}

fn field_value(fields: &[(String, Vec<String>)], labels: &[&str]) -> Option<String> {
    field_values(fields, labels).into_iter().next()
}

fn split_statement_of_responsibility(value: &str) -> (String, Option<String>) {
    match value.split_once(" / ") {
        Some((title, responsibility)) => (title.trim().trim_end_matches(" .").to_owned(),
            non_empty(responsibility.trim().trim_end_matches('.').to_owned())),
        None => (value.trim().trim_end_matches('.').to_owned(), None),
    }
}

fn extract_publication_year(value: &str) -> Option<u16> {
    Regex::new(r"\b(1[5-9]\d{2}|20\d{2})\b").ok()?
        .find_iter(value).filter_map(|m| m.as_str().parse::<u16>().ok()).last()
}

fn strip_publication_year(value: &str) -> String {
    Regex::new(r"\b(?:1[5-9]\d{2}|20\d{2})\b").ok()
        .map(|re| re.replace_all(value, "").into_owned()).unwrap_or_else(|| value.to_owned())
        .trim().trim_end_matches(|character: char| character.is_whitespace() || matches!(character, ',' | '.' | ';' | ':')).trim().to_owned()
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

pub(super) fn holdings(html: &str) -> Vec<CopyHolding> {
    extract_holdings(html)
}

fn library_page_diagnostic(html: &str) -> String {
    let title = Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>").ok()
        .and_then(|re| re.captures(html)).and_then(|captures| captures.get(1))
        .map(|value| clean_text(value.as_str())).filter(|value| !value.is_empty())
        .unwrap_or_else(|| "(no title)".to_owned());
    let selects = Regex::new(r"(?is)<select\b([^>]*)>(.*?)</select>").ok()
        .map(|re| re.captures_iter(html).map(|captures| {
            let attrs = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");
            let options = Regex::new(r"(?is)<option\b").map(|re| re.find_iter(body).count()).unwrap_or(0);
            format!("select[{attrs}] options={options}")
        }).take(8).collect::<Vec<_>>()).unwrap_or_default();
    let forms = Regex::new(r#"(?is)<form\b([^>]*)>"#).ok()
        .map(|re| re.captures_iter(html).filter_map(|captures| captures.get(1).map(|m| m.as_str().trim().to_owned()))
            .take(5).collect::<Vec<_>>()).unwrap_or_default();
    let body = Regex::new(r"(?is)<body\b[^>]*>(.*?)</body>").ok()
        .and_then(|re| re.captures(html)).and_then(|captures| captures.get(1))
        .map(|value| clean_text(value.as_str())).unwrap_or_else(|| clean_text(html));
    let excerpt: String = body.chars().take(240).collect();
    format!("page title={title:?}; selects={selects:?}; forms={forms:?}; body excerpt={excerpt:?}")
}

pub(super) fn libraries(html: &str) -> Result<Vec<Library>, RabelError> {
    let selects = Regex::new(r#"(?is)<select\b([^>]*)>(.*?)</select>"#).map_err(|error| RabelError::Parse(error.to_string()))?;
    let selector_attr = Regex::new(r#"(?i)(?:name|id)\s*=\s*["']subcat["']"#).map_err(|error| RabelError::Parse(error.to_string()))?;
    let options = Regex::new(r#"(?is)<option\b[^>]*value\s*=\s*["']([^"']+)["'][^>]*>(.*?)</option>"#).map_err(|error| RabelError::Parse(error.to_string()))?;
    let select = selects.captures_iter(html)
        .find(|captures| captures.get(1).is_some_and(|attrs| selector_attr.is_match(attrs.as_str())))
        .and_then(|captures| captures.get(2))
        .ok_or_else(|| RabelError::Parse(format!("library selector not found; {}", library_page_diagnostic(html))))?;
    let libraries: Vec<Library> = options.captures_iter(select.as_str()).filter_map(|c| {
        let id = c.get(1)?.as_str().trim().to_owned();
        let raw_name = clean_text(c.get(2)?.as_str());
        let trimmed_name = raw_name.trim_start();
        let depth = trimmed_name.chars().take_while(|character| *character == '>').count();
        let name = trimmed_name[depth..].trim().to_owned();
        (!id.is_empty() && !name.is_empty()).then_some(Library { id, name, depth })
    }).collect();
    if libraries.is_empty() { return Err(RabelError::Parse("RABEL library selector contains no usable options".into())); }
    Ok(libraries)
}

fn extract_holdings(html: &str) -> Vec<CopyHolding> {
    let Ok(rows) = Regex::new(r#"(?is)(<tr\b[^>]*>)(.*?)</tr>"#) else { return Vec::new(); };
    let Ok(status_attr) = Regex::new(r#"(?i)data-disp\s*=\s*["']([^"']+)["']"#) else { return Vec::new(); };
    rows.captures_iter(html).filter_map(|row| {
        let opening_tag = row.get(1)?.as_str();
        let contents = row.get(2)?.as_str();
        let raw = row.get(0)?.as_str();
        let status = status_attr.captures(opening_tag).or_else(|| status_attr.captures(contents))?.get(1)?.as_str();
        let text = clean_text(raw);
        let library = labelled_value(&text, &["Biblioteca", "Sucursal"]);
        let signature = extract_signature(&text).or_else(|| labelled_value(&text, &["Signatura", "SIGN"]));
        let location = labelled_value(&text, &["Localización", "Ubicación"])
            .or_else(|| known_value(&text, &["Depósito", "Sala de préstamo", "Sala de lectura", "Sala infantil"]));
        let material = labelled_value(&text, &["Tipo de ejemplar", "Tipo", "Formato"])
            .or_else(|| known_value(&text, &["Libro adultos", "Libro infantil", "Papel", "DVD", "CD"]));
        let catalogue_note = (library.is_none() && signature.is_none() && location.is_none() && material.is_none() && !text.is_empty()).then_some(text);
        Some(CopyHolding { library, signature, location, material, availability: normalize_availability(status), catalogue_note })
    }).collect()
}

fn extract_signature(text: &str) -> Option<String> {
    Regex::new(r"\b[A-Z]{1,4}-[A-Z0-9]+(?:\s+[A-Z0-9]+)?\b").ok()?.find(text).map(|matched| matched.as_str().to_owned())
}

fn known_value(text: &str, values: &[&str]) -> Option<String> {
    values.iter().find(|value| text.to_lowercase().contains(&value.to_lowercase())).map(|value| (*value).to_owned())
}

fn normalize_availability(value: &str) -> Availability {
    let value = value.trim().to_lowercase();
    if value.contains("no disponible") { Availability::Other }
    else if value.contains("disponible") { Availability::Available }
    else if value.contains("prestado") || value.contains("en préstamo") { Availability::Loaned }
    else if value.contains("consulta") || value.contains("sala") { Availability::ReadingRoomOnly }
    else if value.is_empty() { Availability::Unknown }
    else { Availability::Other }
}


fn extract_document_category(title: &str, row_html: &str) -> Option<String> {
    // RABEL embeds the authoritative document type in the bibliographic title
    // (for example "[Vídeo]" or "[Grabación sonora]"). Use it before icons
    // or optional row labels, which can be generic or theme-dependent.
    // RABEL sometimes emits an unbalanced title marker, e.g.
    // "El agua y la vida . Vídeo]. ... ["; match known classes as tokens.
    let title_marker = Regex::new(
        r"(?i)(?:\[?\s*|\b)(vídeo|video|videograbaciones?|películas?|grabaci[oó]n(?:es)?\s+sonor[ao]s?|recursos?\s+electrónicos?|monografías?|libros?|revistas?|periódicos?|artículos?\s+de\s+revista|material\s+cartográfico|manuscritos?|material\s+gráfico|partituras?)(?:\s*\]|\b)"
    ).ok()?;
    if let Some(value) = title_marker.captures(title).and_then(|capture| capture.get(1)) {
        return Some(normalize_document_category(value.as_str()));
    }

    let text = clean_text(row_html);
    let labelled = labelled_value(&text, &["Tipo de documento", "Tipo de material", "Tipo de soporte", "Material"]);
    if let Some(value) = labelled { return Some(normalize_document_category(&value)); }

    let attributes = Regex::new(r#"(?is)<img\b[^>]*(?:alt|title)\s*=\s*["']([^"']+)["'][^>]*>"#).ok()?;
    attributes.captures_iter(row_html).find_map(|capture| {
        let value = clean_text(capture.get(1)?.as_str());
        let normalized = value.to_lowercase();
        let known = ["book", "monografía", "monografías", "libro", "videograbación", "videograbaciones",
            "vídeo", "video", "película", "grabación sonora", "grabaciones sonoras", "revista",
            "revistas", "periódico", "periódicos", "artículo de revista", "recurso electrónico",
            "material cartográfico", "manuscrito", "material gráfico", "partitura"];
        known.iter().any(|candidate| normalized.contains(candidate)).then(|| normalize_document_category(&value))
    })
}

fn normalize_document_category(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized.contains("monograf") || normalized == "libro" || normalized.contains("libros") {
        "Book".to_owned()
    } else if normalized.contains("videograb") || normalized.contains("película")
        || normalized == "vídeo" || normalized == "video" || normalized.contains("vídeo") {
        "Film / video".to_owned()
    } else if normalized.contains("grabación sonora") || normalized.contains("grabacion sonora") || normalized.contains("grabaciones sonoras") {
        "Audio".to_owned()
    } else if normalized.contains("artículo de revista") {
        "Journal article".to_owned()
    } else if normalized.contains("revista") || normalized.contains("periódico") {
        "Magazine / newspaper".to_owned()
    } else if normalized.contains("recurso electrónico") || normalized.contains("recurso electronico") {
        "Digital resource".to_owned()
    } else if normalized.contains("material cartográfico") || normalized.contains("material cartografico") {
        "Map".to_owned()
    } else if normalized.contains("manuscrito") {
        "Manuscript".to_owned()
    } else if normalized.contains("material gráfico") || normalized.contains("material grafico") {
        "Graphic material".to_owned()
    } else if normalized.contains("partitura") {
        "Sheet music".to_owned()
    } else {
        value.trim().to_owned()
    }
}

fn extract_authors(text: &str) -> Vec<String> {
    labelled_value(text, &["Autor / Autora", "Autor", "Autores", "Autoría", "Autoria"]).map(|value| {
        value.split(';').map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned).collect()
    }).unwrap_or_default()
}

fn extract_isbn(text: &str) -> Option<String> {
    Regex::new(r"(?i)\bISBN\s*:?\s*([0-9X-]{10,17})\b").ok()?
        .captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().to_owned())
}

fn labelled_value(text: &str, labels: &[&str]) -> Option<String> {
    let next_field = Regex::new(r"(?i)\s+(?:Disponibilidad|Soporte|Biblioteca|Sucursal|Signatura|SIGN|Localización|Ubicación|Tipo de ejemplar|Tipo de documento|Tipo de material|Tipo de soporte|Formato|Material|Tipo|Autor(?:es)?|Autor\s*/\s*Autora|Editorial|Editor\s*/\s*Productor|Productor|Publicación|Edición|Descripción\s+física|Colección\s+editorial|Serie|Nota\s+de\s+bibliografía|Notas|Materias|Materia|ISBN)(?:\s*:\s*|\s+)").ok()?;
    for label in labels {
        let pattern = format!(r"(?i){}\s*:?\s*([^|]{{2,120}})", regex::escape(label));
        let re = Regex::new(&pattern).ok()?;
        if let Some(value) = re.captures(text).and_then(|c| c.get(1)) {
            let raw = value.as_str();
            let end = next_field.find(raw).map(|matched| matched.start()).unwrap_or(raw.len());
            let value = raw[..end].trim().trim_matches(|character: char| matches!(character, ':' | '/' | '·' | '|' | ';')).trim().to_owned();
            if !value.is_empty() { return Some(value); }
        }
    }
    None
}

fn surrounding_row(html: &str, offset: usize) -> String {
    let start = html[..offset].rfind("<tr").unwrap_or(offset.saturating_sub(800));
    let end = html[offset..].find("</tr>").map(|n| offset + n + 5).unwrap_or((offset + 800).min(html.len()));
    html[start..end].to_owned()
}

fn clean_text(html: &str) -> String {
    let without_tags = Regex::new(r"(?is)<script\b.*?</script>|<style\b.*?</style>|<[^>]+>")
        .map(|re| re.replace_all(html, " ").into_owned()).unwrap_or_else(|_| html.to_owned());
    decode_entities(&without_tags).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(value: &str) -> String {
    value.replace("&nbsp;", " ").replace("&amp;", "&").replace("&quot;", "\"")
        .replace("&#39;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&aacute;", "á").replace("&eacute;", "é").replace("&iacute;", "í")
        .replace("&oacute;", "ó").replace("&uacute;", "ú").replace("&ntilde;", "ñ")
        .replace("&Aacute;", "Á").replace("&Eacute;", "É").replace("&Iacute;", "Í")
        .replace("&Oacute;", "Ó").replace("&Uacute;", "Ú").replace("&Ntilde;", "Ñ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_rabel_bibliography_without_confusing_author_dates_for_edition_year() {
        let html = r#"
            <h2>Menú responsive</h2>
            <div id="docu_body" class="docu_body"><div class="docu_etiq">
              <dl><dt>Autor / Autora:</dt><dd><a>Hayek, Friedrich A. von (1899-1992)</a></dd></dl>
              <dl><dt>Título:</dt><dd><span>Los fundamentos de la libertad</span> / F. A. Hayek.</dd></dl>
              <dl><dt>Editor / Productor:</dt><dd>Valencia : Fundación Ignacio Villalonga, 1961.</dd></dl>
              <dl><dt>Descripción física:</dt><dd>454 p. ; 22 cm.</dd></dl>
              <dl><dt>Colección editorial:</dt><dd>Biblioteca de estudios económicos</dd><dd>Biblioteca de Estudios Económicos</dd></dl>
              <dl><dt>Nota de bibliografía:</dt><dd>Índice de autores.</dd></dl>
              <dl><dt>Materias:</dt><dd>Libertad</dd><dd>Política económica</dd><dd>Derechos humanos</dd><dd>Política social</dd></dl>
              <dl><dt>ISBN:</dt><dd>84-254-1151-3</dd></dl>
            </div></div>"#;
        let item = record(html).unwrap();
        assert_eq!(item.title.as_deref(), Some("Los fundamentos de la libertad"));
        assert_eq!(item.authors, vec!["Hayek, Friedrich A. von (1899-1992)"]);
        assert_eq!(item.responsibility.as_deref(), Some("F. A. Hayek"));
        assert_eq!(item.publisher.as_deref(), Some("Valencia : Fundación Ignacio Villalonga"));
        assert_eq!(item.publication_year, Some(1961));
        assert_eq!(item.publication_statement.as_deref(), Some("Valencia : Fundación Ignacio Villalonga, 1961."));
        assert_eq!(item.physical_description.as_deref(), Some("454 p. ; 22 cm."));
        assert_eq!(item.series.as_deref(), Some("Biblioteca de estudios económicos"));
        assert_eq!(item.notes, vec!["Índice de autores."]);
        assert_eq!(item.subjects, vec!["Libertad", "Política económica", "Derechos humanos", "Política social"]);
        assert_eq!(item.isbn.as_deref(), Some("84-254-1151-3"));
    }

    #[test]
    fn record_details_contain_bibliographic_data_only() {
        let html = r#"<html><h1>Menú responsive</h1><div id="docu_body">
            <dl><dt>Título:</dt><dd>The Example Book</dd></dl>
            <dl><dt>Autor:</dt><dd>Ada Lovelace</dd></dl>
            <dl><dt>Editorial:</dt><dd>Example Press</dd></dl>
            <dl><dt>ISBN:</dt><dd>978-1234567890</dd></dl>
            </div><table><tr data-disp="Disponible"><td>Signatura D-123 Depósito Libro adultos Papel</td></tr></table>
        </html>"#;
        let item = record(html).unwrap();
        assert_eq!(item.title.as_deref(), Some("The Example Book"));
        assert_eq!(item.authors, vec!["Ada Lovelace"]);
        assert_eq!(item.publisher.as_deref(), Some("Example Press"));
        assert_eq!(item.isbn.as_deref(), Some("978-1234567890"));
    }

    #[test]
    fn recovers_dated_author_from_search_heading_when_row_has_no_author_field() {
        assert_eq!(
            split_dated_author_prefix("Hayek, Friedrich A. von (1899-1992) Los fundamentos de la libertad"),
            Some((
                "Hayek, Friedrich A. von (1899-1992)".to_owned(),
                "Los fundamentos de la libertad".to_owned()
            ))
        );
    }

    #[test]
    fn search_results_include_bibliographic_details_without_record_fetch() {
        let html = r#"<table><tr>
            <td><a href="/token/NT1?ACC=165&amp;DOC=42">Hayek, Friedrich A. von (1899-1992) Los fundamentos de la libertad / Friedrich A. Hayek.</a></td>
            <td>Autor / Autora: Hayek, Friedrich A. von (1899-1992)</td>
            <td>Editor / Productor: Valencia : Fundación Ignacio Villalonga, 1961.</td>
            <td>Descripción física: 454 p. ; 22 cm.</td>
            <td>Colección editorial: Biblioteca de estudios económicos</td>
            <td>Nota de bibliografía: Índice de autores.</td>
            <td>Materias: Libertad</td>
            <td>ISBN: 84-254-1151-3</td>
        </tr></table>"#;
        let session = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token/NT1").unwrap();
        let results = search_results(html, &session).unwrap();
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert_eq!(result.title, "Los fundamentos de la libertad");
        assert_eq!(result.authors, vec!["Hayek, Friedrich A. von (1899-1992)"]);
        assert_eq!(result.responsibility.as_deref(), Some("Friedrich A. Hayek"));
        assert_eq!(result.publisher.as_deref(), Some("Valencia : Fundación Ignacio Villalonga"));
        assert_eq!(result.publication_year, Some(1961));
        assert_eq!(result.physical_description.as_deref(), Some("454 p. ; 22 cm."));
        assert_eq!(result.series.as_deref(), Some("Biblioteca de estudios económicos"));
        assert_eq!(result.notes, vec!["Índice de autores."]);
        assert_eq!(result.subjects, vec!["Libertad"]);
        assert_eq!(result.isbn.as_deref(), Some("84-254-1151-3"));
    }

    #[test]
    fn recognizes_common_empty_catalogue_responses() {
        for message in ["No se han encontrado registros", "No hay resultados para la búsqueda", "0 documentos"] {
            let html = format!("<html><body>{message}</body></html>");
            let session = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token/NT1").unwrap();
            assert!(search_results(&html, &session).unwrap().is_empty(), "{message}");
        }
    }

    #[test]
    fn extracts_document_category_from_title_markers() {
        let cases = [
            ("Aardman classics [Vídeo] / scénario", "Film / video"),
            ("Adams, Bryan Anthology [Grabación sonora] / Bryan Adams", "Audio"),
            ("Agassi tennis generation [Recurso electrónico] / developed by Aqua Pacific", "Digital resource"),
            ("La trampa de la diversidad / Daniel Bernabé", "Book"),
        ];
        for (title, expected) in cases {
            assert_eq!(extract_document_category(title, "<tr><td><img alt=\"Book\"></td></tr>").as_deref(), Some(expected));
        }
    }

    #[test]
    fn title_marker_takes_precedence_over_generic_book_icon() {
        assert_eq!(extract_document_category("Al rojo vivo [Vídeo]", r#"<tr><td><img alt="Book"></td></tr>"#).as_deref(), Some("Film / video"));
    }

    #[test]
    fn extracts_document_category_from_catalogue_row() {
        let html = r#"<tr><td><a href="/token/NT1?ACC=165&amp;DOC=42">El viaje</a></td><td>Tipo de documento: Videograbaciones</td></tr>"#;
        let session = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token/NT1").unwrap();
        let results = search_results(html, &session).unwrap();
        assert_eq!(results[0].category.as_deref(), Some("Film / video"));
    }

    #[test]
    fn extracts_category_from_accessible_document_icon() {
        let html = r#"<tr><td><a href="/token/NT1?ACC=165&amp;DOC=43">La película</a><img title="Videograbaciones" src="video.gif"></td></tr>"#;
        let session = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token/NT1").unwrap();
        let results = search_results(&html, &session).unwrap();
        assert_eq!(results[0].category.as_deref(), Some("Film / video"));
    }

    #[test]
    fn recognizes_unbalanced_rabel_video_marker_in_title() {
        assert_eq!(
            extract_document_category("El agua y la vida . Vídeo]. El agua en el planeta [", "<tr><td><img alt=\"Book\"></td></tr>").as_deref(),
            Some("Film / video")
        );
    }

    #[test]
    fn title_document_marker_takes_precedence_over_generic_book_icon() {
        for (title, expected) in [
            ("El agua y la vida . Vídeo]. El agua en el planeta [", "Film / video"),
            ("Adams, Bryan Anthology [Grabación sonora]", "Audio"),
            ("Agassi tennis generation [Recurso electrónico]", "Digital resource"),
            ("Agenda 2001 : convivencia, igualdad, tolerancia", "Book"),
        ] {
            assert_eq!(
                extract_document_category(title, "<tr><td><img alt=\"Book\"></td></tr>").as_deref(),
                Some(expected),
                "{title}"
            );
        }
    }

    #[test]
    fn maps_monographs_to_books_and_audio_to_audio() {
        assert_eq!(normalize_document_category("Monografías"), "Book");
        assert_eq!(normalize_document_category("Grabaciones sonoras"), "Audio");
    }

    #[test]
    fn parses_library_selector_options() {
        let html = r#"<form><select onchange="ChgSubcat();" id="subcat" name="subcat">
            <option value="SAL/BPSA">Biblioteca Pública de Salamanca</option>
            <option value="SAL/BPBE">Biblioteca Pública de Béjar</option>
        </select></form>"#;
        let libraries = libraries(html).unwrap();
        assert_eq!(libraries.len(), 2);
        assert_eq!(libraries[0].id, "SAL/BPSA");
        assert_eq!(libraries[0].name, "Biblioteca Pública de Salamanca");
        assert_eq!(libraries[1].id, "SAL/BPBE");
    }

    #[test]
    fn parses_library_hierarchy_markers_as_depth() {
        let html = r#"<select id="subcat" name="subcat">
            <option value="SAL">Salamanca</option>
            <option value="SAL/BPSA">&gt;SALAMANCA. Biblioteca Pública de Salamanca</option>
            <option value="SAL/BPSA-1">&gt;&gt;Pública de Salamanca</option>
            <option value="SAL/ARCH">&gt;Archivos de Salamanca</option>
            <option value="SAL/ARCH/BIB">&gt;&gt;SALAMANCA. Archivo Histórico Provincial. Biblioteca</option>
        </select>"#;
        let libraries = libraries(html).unwrap();
        assert_eq!(libraries.len(), 5);
        assert_eq!((libraries[0].name.as_str(), libraries[0].depth), ("Salamanca", 0));
        assert_eq!((libraries[1].name.as_str(), libraries[1].depth), ("SALAMANCA. Biblioteca Pública de Salamanca", 1));
        assert_eq!((libraries[2].name.as_str(), libraries[2].depth), ("Pública de Salamanca", 2));
        assert_eq!((libraries[3].name.as_str(), libraries[3].depth), ("Archivos de Salamanca", 1));
        assert_eq!((libraries[4].name.as_str(), libraries[4].depth), ("SALAMANCA. Archivo Histórico Provincial. Biblioteca", 2));
    }

    #[test]
    fn missing_library_selector_is_a_parse_error() {
        let error = libraries("<html><body>Welcome</body></html>").unwrap_err();
        assert!(error.to_string().contains("library selector not found"));
        assert!(error.to_string().contains("page title"));
        assert!(error.to_string().contains("body excerpt"));
    }

    #[test]
    fn extracts_session_endpoint_from_landing_form() {
        let html = r#"<form id="abnformSearch" action="/cgi-bin/abnetopac2/token/NT1" method="post"></form>"#;
        let base = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token").unwrap();
        let endpoint = session_action(html, &base).unwrap();
        assert_eq!(endpoint.as_str(), "https://rabel.jcyl.es/cgi-bin/abnetopac2/token/NT1");
    }

    #[test]
    fn parses_meta_refresh_and_search_links() {
        let html = r#"<meta http-equiv="Refresh" content="0; URL=/cgi-bin/abnetopac2/token?ACC=161"><tr><td><a href="/cgi-bin/abnetopac2/token/NT1?ACC=165&amp;DOC=2">El Quijote</a></td></tr>"#;
        let session = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token?ACC=161").unwrap();
        assert!(meta_refresh_target(html).unwrap().contains("token"));
        let results = search_results(html, &session).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "El Quijote");
        assert!(results[0].record_ref.contains("DOC=2"));
    }

    #[test]
    fn parses_copy_status_when_data_disp_is_on_the_table_row() {
        let html = r#"<table><tr data-disp="Disponible"><td>D-18214 Depósito Libro adultos Papel</td></tr></table>"#;
        let holdings = extract_holdings(html);
        assert_eq!(holdings.len(), 1);
        assert_eq!(holdings[0].availability, Availability::Available);
        assert_eq!(holdings[0].signature.as_deref(), Some("D-18214"));
    }

    #[test]
    fn parses_deposit_signature_without_repeating_the_raw_row() {
        let html = r#"<table><tr data-disp="Disponible"><td>Signatura Depósito D-2 5331 Tipo de ejemplar Libro adultos Soporte Papel Disponibilidad Disponible</td></tr></table>"#;
        let holdings = extract_holdings(html);
        assert_eq!(holdings.len(), 1);
        assert_eq!(holdings[0].signature.as_deref(), Some("D-2 5331"));
        assert_eq!(holdings[0].location.as_deref(), Some("Depósito"));
        assert_eq!(holdings[0].material.as_deref(), Some("Libro adultos"));
        assert_eq!(holdings[0].availability, Availability::Available);
        assert_eq!(holdings[0].catalogue_note, None);
    }

    #[test]
    fn no_disponible_is_not_available() {
        assert_eq!(normalize_availability("No disponible"), Availability::Other);
    }

    #[test]
    fn unknown_copy_status_is_never_available() {
        let html = r#"<table><tr><td data-disp="Estado raro">D-123 Depósito</td></tr></table>"#;
        let holdings = extract_holdings(html);
        assert_eq!(holdings.len(), 1);
        assert_eq!(holdings[0].availability, Availability::Other);
    }

    #[test]
    fn parses_available_copy_rows() {
        let html = r#"<table><tr><td data-disp="Disponible">D-18214 Depósito Libro adultos Papel</td></tr></table>"#;
        let holdings = extract_holdings(html);
        assert_eq!(holdings[0].availability, Availability::Available);
        assert_eq!(holdings[0].signature.as_deref(), Some("D-18214"));
        assert_eq!(holdings[0].location.as_deref(), Some("Depósito"));
        assert_eq!(holdings[0].material.as_deref(), Some("Libro adultos"));
        assert_eq!(holdings[0].catalogue_note, None);
    }
}
