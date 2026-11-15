mod types;
use types::*;

//! Alexa text-to-speech (TTS) implementation.
//!
//! This module is a Rust port of the TTS-related logic from
//! [alexa-remote-control](https://github.com/thorsten-gehrig/alexa-remote-control).
//! It follows that project's flow to authenticate with Amazon Alexa, resolve the
//! target Echo device, and submit an `Alexa.Speak` command.
//!
//! This is intentionally a focused port of the TTS functionality, not a
//! reimplementation of the full alexa-remote-control script or a general-purpose
//! Alexa API client.
//!
//! The upstream project remains the reference for the underlying Alexa Remote
//! API interactions and behavior.

use reqwest::{
    Client,
    header::{CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, ORIGIN, REFERER, USER_AGENT},
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use thiserror::Error;

const AMAZON: &str = "amazon.es";
const ALEXA: &str = "alexa.amazon.es";
const TTS_LOCALE: &str = "es-ES";
const USER_AGENT_STR: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:1.0) bash-script/1.0";

#[derive(Error, Debug)]
pub enum AlexaError {
    #[error("AMAZON_ALEXA_REFRESH_TOKEN env var not set")]
    MissingToken,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Auth failed: {0}")]
    Auth(String),
    #[error("Device '{0}' not found")]
    DeviceNotFound(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct CookieEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct ExchangeResponse {
    response: ExchangeInner,
}

#[derive(Debug, Deserialize)]
struct ExchangeInner {
    tokens: Tokens,
}

#[derive(Debug, Deserialize)]
struct Tokens {
    cookies: HashMap<String, Vec<CookieEntry>>,
}

#[derive(Debug, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

#[derive(Debug, Deserialize)]
struct Device {
    #[serde(rename = "accountName")]
    account_name: String,
    #[serde(rename = "deviceType")]
    device_type: String,
    #[serde(rename = "serialNumber")]
    serial_number: String,
    #[serde(rename = "deviceFamily")]
    #[allow(dead_code)]
    device_family: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserMe {
    id: String,
}

/// Envía un TTS (Alexa.Speak) a un dispositivo Echo.
///
/// - `device_name`: nombre exacto del Echo (el que sale en la app / `-a` del script)
/// - `text`: texto plano o SSML (sin envolver en <speak> a menos que quieras SSML completo)
pub async fn alexa_speak(device_name: &str, text: &str) -> Result<(), AlexaError> {
    let refresh_token =
        std::env::var("AMAZON_ALEXA_REFRESH_TOKEN").map_err(|_| AlexaError::MissingToken)?;

    let client = Client::builder()
        .cookie_store(true)
        .user_agent(USER_AGENT_STR)
        .build()?;

    // 1) Intercambiar refresh_token por cookies de sesión
    let mut cookie_header = exchange_refresh_token(&client, &refresh_token).await?;

    // 2) Obtener CSRF (y validar sesión)
    let csrf = fetch_csrf(&client, &cookie_header).await?;

    // Include the CSRF cookie in subsequent requests.
    cookie_header.push_str(&format!("; csrf={csrf}"));

    // 3) Resolver dispositivo + customerId
    let (device_type, serial, customer_id) =
        resolve_device(&client, &cookie_header, &csrf, device_name).await?;

    // 4) Enviar Alexa.Speak
    send_speak(
        &client,
        &cookie_header,
        &csrf,
        &device_type,
        &serial,
        &customer_id,
        text,
    )
    .await
}

async fn exchange_refresh_token(
    client: &Client,
    refresh_token: &str,
) -> Result<String, AlexaError> {
    let url = format!("https://api.{AMAZON}/ap/exchangetoken/cookies");
    let domain = format!("www.{AMAZON}");

    let mut form = HashMap::new();
    form.insert("app_name", "Amazon Alexa");
    form.insert("requested_token_type", "auth_cookies");
    form.insert("domain", domain.as_str());
    form.insert("source_token_type", "refresh_token");
    form.insert("source_token", refresh_token);

    let res = client
        .post(&url)
        .header("x-amzn-identity-auth-domain", format!("api.{AMAZON}"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await?
        .error_for_status()?;

    let body: ExchangeResponse = res.json().await?;

    // Preferimos el dominio exacto; si no, el primero disponible
    let cookies = body
        .response
        .tokens
        .cookies
        .get(&format!(".{AMAZON}"))
        .or_else(|| body.response.tokens.cookies.values().next())
        .ok_or_else(|| AlexaError::Auth("no cookies in exchange response".into()))?;

    let cookie_header = cookies
        .iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ");

    // Comprobación mínima de que hay un token de sesión
    if !cookie_header.contains("at-") && !cookie_header.contains("session-token") {
        return Err(AlexaError::Auth(
            "exchange did not return a usable session cookie".into(),
        ));
    }

    Ok(cookie_header)
}

async fn fetch_csrf(client: &Client, cookie_header: &str) -> Result<String, AlexaError> {
    // Cualquiera de estos endpoints suele setear la cookie csrf
    let endpoints = [
        format!("https://{ALEXA}/api/language"),
        format!("https://{ALEXA}/api/devices-v2/device?cached=false"),
    ];

    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(cookie_header).unwrap());
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://alexa.amazon.es/spa/index.html"),
    );
    headers.insert(ORIGIN, HeaderValue::from_static("https://alexa.amazon.es"));

    for url in &endpoints {
        let res = client.get(url).headers(headers.clone()).send().await?;

        // reqwest cookie_store debería capturar Set-Cookie, pero también
        // miramos el header por si acaso
        if let Some(set_cookie) = res.headers().get_all("set-cookie").iter().find_map(|v| {
            let s = v.to_str().ok()?;
            if s.contains("csrf=") {
                Some(s.to_string())
            } else {
                None
            }
        }) {
            if let Some(csrf) = extract_csrf_value(&set_cookie) {
                return Ok(csrf);
            }
        }
    }

    // Fallback: a veces el csrf viene solo en el cookie jar del client.
    // Si usas cookie_store, puedes leerlo de ahí. Aquí devolvemos error claro.
    Err(AlexaError::Auth(
        "could not obtain csrf cookie (try hitting /api/language manually)".into(),
    ))
}

fn extract_csrf_value(set_cookie: &str) -> Option<String> {
    for part in set_cookie.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("csrf=") {
            return Some(v.to_string());
        }
    }
    None
}

async fn resolve_device(
    client: &Client,
    cookie_header: &str,
    csrf: &str,
    device_name: &str,
) -> Result<(String, String, String), AlexaError> {
    let headers = common_headers(cookie_header, csrf);

    // customerId
    let me: UserMe = client
        .get(format!("https://{ALEXA}/api/users/me"))
        .headers(headers.clone())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // device list
    let devices: DevicesResponse = client
        .get(format!(
            "https://{ALEXA}/api/devices-v2/device?cached=false"
        ))
        .headers(headers)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let device = devices
        .devices
        .into_iter()
        .find(|d| d.account_name.eq_ignore_ascii_case(device_name))
        .ok_or_else(|| AlexaError::DeviceNotFound(device_name.to_string()))?;

    Ok((device.device_type, device.serial_number, me.id))
}

async fn send_speak(
    client: &Client,
    cookie_header: &str,
    csrf: &str,
    device_type: &str,
    serial: &str,
    customer_id: &str,
    text: &str,
) -> Result<(), AlexaError> {
    // Payload equivalente al SEQUENCECMD=Alexa.Speak del script
    let sequence = json!({
        "@type": "com.amazon.alexa.behaviors.model.Sequence",
        "startNode": {
            "@type": "com.amazon.alexa.behaviors.model.ParallelNode",
            "nodesToExecute": [
                {
                    "@type": "com.amazon.alexa.behaviors.model.OpaquePayloadOperationNode",
                    "type": "Alexa.Speak",
                    "operationPayload": {
                        "deviceType": device_type,
                        "deviceSerialNumber": serial,
                        "customerId": customer_id,
                        "locale": TTS_LOCALE,
                        "textToSpeak": text
                    }
                }
            ]
        }
    });

    let body = json!({
        "behaviorId": "PREVIEW",
        "sequenceJson": serde_json::to_string(&sequence)?,
        "status": "ENABLED"
    });

    let res = client
        .post(format!("https://{ALEXA}/api/behaviors/preview"))
        .headers(common_headers(cookie_header, csrf))
        .json(&body)
        .send()
        .await?;

    let status = res.status();
    let text_body = res.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(AlexaError::Api(format!(
            "behaviors/preview returned {status}: {text_body}"
        )));
    }

    Ok(())
}

fn common_headers(cookie_header: &str, csrf: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(COOKIE, HeaderValue::from_str(cookie_header).unwrap());
    h.insert("csrf", HeaderValue::from_str(csrf).unwrap());
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=UTF-8"),
    );
    h.insert(
        REFERER,
        HeaderValue::from_static("https://alexa.amazon.es/spa/index.html"),
    );
    h.insert(ORIGIN, HeaderValue::from_static("https://alexa.amazon.es"));
    h.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_STR));
    h
}
