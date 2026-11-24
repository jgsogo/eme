//! Command-line probe for the live Salamanca RABEL catalogue.

use std::{env, error::Error, fs};

use rabel_tool::RabelClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let query = args.next().ok_or(
        "usage: cargo run -p rabel-tool --bin rabel-poc -- <query> [output.html]",
    )?;
    let output = args
        .next()
        .unwrap_or_else(|| "rabel-search.html".to_owned());

    let client = RabelClient::new()?;
    let response = client.search(&query).await?;

    fs::write(&output, &response.html)?;

    println!("HTTP status: {}", response.status);
    println!(
        "Content-Type: {}",
        response.content_type.as_deref().unwrap_or("<not provided>")
    );
    println!("Final URL: {}", response.final_url);
    println!("Response size: {} bytes", response.html.len());
    println!("Saved HTML: {output}");

    if !(200..300).contains(&response.status) {
        return Err(format!("RABEL returned HTTP {}", response.status).into());
    }

    Ok(())
}
