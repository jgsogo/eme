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

pub(super) fn search_results(html: &str, session_url: &Url) -> Result<Vec<CatalogueSearchResult>, RabelError> {
    let link = Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#)
        .map_err(|error| RabelError::Parse(error.to_string()))?;
    let mut results = Vec::new();
    for captures in link.captures_iter(html) {
        let href = captures.get(1).unwrap().as_str();
        if !href.contains("ACC=165") || !href.contains("DOC=") {
            continue;
        }
        let record_url = session_url.join(href)
            .map_err(|error| RabelError::Parse(format!("invalid record link: {error}")))?;
        let title = clean_text(captures.get(2).unwrap().as_str());
        if title.is_empty() { continue; }
        let context = surrounding_row(html, captures.get(0).unwrap().start());
        results.push(CatalogueSearchResult {
            record_ref: record_url.to_string(),
            title,
            authors: extract_authors(&context),
            publication: labelled_value(&context, &["Publicación", "Edición", "Datos de publicación"]),
            isbn: extract_isbn(&context),
        });
    }
    if results.is_empty() && !html.to_ascii_lowercase().contains("no se han encontrado") {
        return Err(RabelError::Parse("no record links found in RABEL search response".into()));
    }
    Ok(results)
}

pub(super) fn record(html: &str, source_url: &str) -> Result<CatalogueItem, RabelError> {
    let text = clean_text(html);
    if text.is_empty() {
        return Err(RabelError::Parse("empty record page".into()));
    }
    let title = first_heading(html).or_else(|| labelled_value(&text, &["Título"]));
    let authors = extract_authors(&text);
    let publisher = labelled_value(&text, &["Editorial", "Publicación"]);
    let publication_year = Regex::new(r"\b(1[5-9]\d{2}|20\d{2})\b")
        .ok().and_then(|re| re.find(&text)).and_then(|m| m.as_str().parse().ok());
    let isbn = extract_isbn(&text);
    let holdings = extract_holdings(html);
    Ok(CatalogueItem {
        title, authors, publisher, publication_year, isbn, holdings,
        checked_at: chrono::Utc::now(), source_url: source_url.to_owned(),
    })
}

pub(super) fn libraries(html: &str) -> Result<Vec<Library>, RabelError> {
    let selects = Regex::new(r#"(?is)<select\b[^>]*(?:name|id)\s*=\s*["'](?:SUBC|subc)["'][^>]*>(.*?)</select>"#)
        .map_err(|error| RabelError::Parse(error.to_string()))?;
    let options = Regex::new(r#"(?is)<option\b[^>]*value\s*=\s*["']([^"']+)["'][^>]*>(.*?)</option>"#)
        .map_err(|error| RabelError::Parse(error.to_string()))?;
    let select = selects.captures(html).and_then(|c| c.get(1))
        .ok_or_else(|| RabelError::Parse("library selector not found in RABEL session page".into()))?;
    let libraries = options.captures_iter(select.as_str()).filter_map(|c| {
        let id = c.get(1)?.as_str().trim().to_owned();
        let name = clean_text(c.get(2)?.as_str());
        (!id.is_empty() && !name.is_empty()).then_some(Library { id, name })
    }).collect();
    Ok(libraries)
}

fn extract_holdings(html: &str) -> Vec<CopyHolding> {
    let Ok(rows) = Regex::new(r#"(?is)<tr\b[^>]*>(.*?)</tr>"#) else { return Vec::new(); };
    let Ok(status_attr) = Regex::new(r#"(?i)data-disp\s*=\s*["']([^"']+)["']"#) else { return Vec::new(); };
    rows.captures_iter(html).filter_map(|row| {
        let raw = row.get(1)?.as_str();
        let status = status_attr.captures(raw)?.get(1)?.as_str();
        let text = clean_text(raw);
            Some(CopyHolding {
            library: labelled_value(&text, &["Biblioteca", "Sucursal"]),
            signature: labelled_value(&text, &["Signatura", "SIGN"])
                .or_else(|| extract_signature(&text)),
            location: labelled_value(&text, &["Localización", "Ubicación"])
                .or_else(|| known_value(&text, &["Depósito", "Sala de préstamo", "Sala de lectura", "Sala infantil"])),
            material: labelled_value(&text, &["Tipo de ejemplar", "Tipo", "Formato"])
                .or_else(|| known_value(&text, &["Libro adultos", "Libro infantil", "Papel", "DVD", "CD"])),
            availability: normalize_availability(status),
            catalogue_note: (!text.is_empty()).then_some(text),
        })
    }).collect()
}

fn extract_signature(text: &str) -> Option<String> {
    Regex::new(r"\b[A-Z]{1,4}-[A-Z0-9-]{2,20}\b")
        .ok()?
        .find(text)
        .map(|matched| matched.as_str().to_owned())
}

fn known_value(text: &str, values: &[&str]) -> Option<String> {
    values.iter().find(|value| text.to_lowercase().contains(&value.to_lowercase()))
        .map(|value| (*value).to_owned())
}

fn normalize_availability(value: &str) -> Availability {
    let value = value.trim().to_lowercase();
    if value.contains("disponible") { Availability::Available }
    else if value.contains("prestado") || value.contains("en préstamo") { Availability::Loaned }
    else if value.contains("consulta") || value.contains("sala") { Availability::ReadingRoomOnly }
    else if value.is_empty() { Availability::Unknown }
    else { Availability::Other }
}

fn first_heading(html: &str) -> Option<String> {
    let re = Regex::new(r#"(?is)<h[1-3]\b[^>]*>(.*?)</h[1-3]>"#).ok()?;
    re.captures(html)
        .and_then(|captures| captures.get(1))
        .map(|capture| clean_text(capture.as_str()))
        .filter(|value| !value.is_empty())
}

fn extract_authors(text: &str) -> Vec<String> {
    labelled_value(text, &["Autor", "Autores"]).map(|value| {
        value.split(';').map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned).collect()
    }).unwrap_or_default()
}

fn extract_isbn(text: &str) -> Option<String> {
    Regex::new(r"(?i)\bISBN\s*:?\s*([0-9X-]{10,17})\b").ok()?
        .captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().to_owned())
}

fn labelled_value(text: &str, labels: &[&str]) -> Option<String> {
    for label in labels {
        let pattern = format!(r"(?i){}\s*:?\s*([^|;]{{2,120}})", regex::escape(label));
        let re = Regex::new(&pattern).ok()?;
        if let Some(value) = re.captures(text).and_then(|c| c.get(1)) {
            let value = value.as_str().trim().trim_matches(':').to_owned();
            if !value.is_empty() { return Some(value); }
        }
    }
    None
}

fn surrounding_row(html: &str, offset: usize) -> String {
    let start = html[..offset].rfind("<tr").unwrap_or(offset.saturating_sub(800));
    let end = html[offset..].find("</tr>").map(|n| offset + n + 5).unwrap_or((offset + 800).min(html.len()));
    clean_text(&html[start..end])
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
    fn parses_library_selector_options() {
        let html = r#"<form><select name="SUBC">
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
    fn missing_library_selector_is_a_parse_error() {
        let error = libraries("<html><body>Welcome</body></html>").unwrap_err();
        assert!(error.to_string().contains("library selector not found"));
    }

    #[test]
    fn parses_meta_refresh_and_search_links() {
        let html = r#"<meta http-equiv="Refresh" content="0; URL=/cgi-bin/abnetopac2/token?ACC=161"><tr><td><a href="/cgi-bin/abnetopac2/token/NT1?ACC=165&DOC=2">El Quijote</a></td></tr>"#;
        let session = Url::parse("https://rabel.jcyl.es/cgi-bin/abnetopac2/token?ACC=161").unwrap();
        let target = meta_refresh_target(html).unwrap();
        assert!(target.contains("token"));
        let results = search_results(html, &session).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "El Quijote");
        assert!(results[0].record_ref.contains("DOC=2"));
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
        assert!(holdings[0].catalogue_note.as_deref().unwrap().contains("D-18214"));
    }
}
