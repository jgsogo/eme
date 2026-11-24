//! Command-line probe for the live Salamanca RABEL catalogue.

use std::{env, error::Error, fs};

use rabel_tool::RabelClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let query = args.next().ok_or(
        "usage: cargo run -p rabel-tool --bin rabel-poc -- <query> [search.html] [document-number]",
    )?;
    let output = args
        .next()
        .unwrap_or_else(|| "rabel-search.html".to_owned());
    let document = args.next().map(|value| value.parse::<u32>()).transpose()?;

    let client = RabelClient::new()?;
    let search = client.search(&query).await?;

    fs::write(&output, &search.html)?;

    println!("Search HTTP status: {}", search.status);
    println!(
        "Content-Type: {}",
        search.content_type.as_deref().unwrap_or("<not provided>")
    );
    println!("Final URL: {}", search.final_url);
    println!("Response size: {} bytes", search.html.len());
    println!("Saved search HTML: {output}");

    if !(200..300).contains(&search.status) {
        return Err(format!("RABEL returned HTTP {}", search.status).into());
    }

    if let Some(document) = document {
        let session_url = search
            .session_url
            .as_deref()
            .ok_or("RABEL search response did not contain a session URL")?;
        let record = client.record(session_url, document).await?;
        let record_output = format!("rabel-record-{document}.html");
        fs::write(&record_output, &record.html)?;

        println!("Record HTTP status: {}", record.status);
        println!("Record URL: {}", record.final_url);
        println!("Record response size: {} bytes", record.html.len());
        println!("Saved record HTML: {record_output}");

        if !(200..300).contains(&record.status) {
            return Err(format!("RABEL returned HTTP {}", record.status).into());
        }
    } else {
        println!("No document number supplied; skipping record-detail request.");
        println!("Pass a third argument (for example, 1) to fetch a record.");
    }

    Ok(())
}
