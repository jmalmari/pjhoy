mod models;
mod config;
mod client;
mod calendar;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use crate::models::TrashService;
use crate::client::PjhoyClient;
use crate::config::load_config;

const SERVICES_FILE: &str = "services.json";
const SERVICES_FULL_FILE: &str = "services_full.json";

#[derive(Parser, Debug)]
#[command(name = "pjhoy")]
#[command(about = "Pirkanmaan Jätehuolto Oy utility", long_about = None)]
struct Cli {
    /// Output ICS calendar file path
    #[arg(long, short, default_value = "pjhoy.ics")]
    output: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Login to PJHOY extranet and save session cookies
    Login,
    /// Fetch trash schedule and update calendar
    Fetch {
        /// Save parsed services JSON to current directory
        #[arg(long = "save-json", short = 'j')]
        save_parsed: bool,

        /// Save original raw JSON response to current directory
        #[arg(long = "save-original-json", short = 'r')]
        save_original: bool,
    },
    /// Generate ICS calendar from current data
    Calendar,
}

/// Load trash schedule from trash_schedule.json file in current directory
fn load_trash_services() -> Result<Vec<TrashService>> {
    if !std::path::Path::new(SERVICES_FILE).exists() {
        return Err(anyhow::anyhow!("{SERVICES_FILE} not found in current directory"));
    }

    let schedule_data = std::fs::read_to_string(SERVICES_FILE)
        .context(format!("Failed to read {SERVICES_FILE}"))?;

    let services: Vec<TrashService> = serde_json::from_str(&schedule_data)
        .context(format!("Failed to parse {SERVICES_FILE}"))?;

    Ok(services)
}

/// Save the parsed services JSON to the schedule file in the current directory
async fn save_parsed_json(services: &[TrashService]) -> Result<()> {
    let json_string = serde_json::to_string_pretty(services)
        .context("Failed to serialize parsed services to JSON")?;

    std::fs::write(SERVICES_FILE, json_string)
        .context(format!("Failed to write JSON to {SERVICES_FILE}"))?;

    println!("Parsed services JSON saved to: {SERVICES_FILE}");

    Ok(())
}

/// Save the raw JSON response to a file in the current directory
async fn save_raw_json(raw_json: &serde_json::Value, filename: &str) -> Result<()> {
    let json_string = serde_json::to_string_pretty(raw_json)
        .context("Failed to serialize raw JSON to string")?;

    std::fs::write(filename, json_string)
        .context(format!("Failed to write JSON to {}", filename))?;

    println!("Original raw JSON data saved to: {}", filename);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup state
    let proj_dirs = config::get_project_dirs()?;
    let config_dir = proj_dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&config_dir).context("Could not create config directory")?;

    let data_dir = proj_dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&data_dir).context("Could not create data directory")?;

    let config = load_config(&config_dir)?;
    let mut client = PjhoyClient::new(config, data_dir)?;
    match cli.command {
        Commands::Login => {
            client.login().await?;
            println!("Login successful and cookies saved.");
        }
        Commands::Fetch { save_parsed, save_original } => {
            let services_json = client.fetch_trash_services().await?;
            let services: Vec<TrashService> = serde_json::from_value(services_json.clone())?;

            println!("Fetched {} trash services", services.len());

            let calendar = calendar::generate_calendar(&services)?;

            // Save calendar file
            let calendar_content = calendar.to_string();
            std::fs::write(&cli.output, calendar_content)
                .context("Failed to write calendar file")?;
            println!("Calendar saved to: {:?}", cli.output);

            // Save parsed JSON if requested
            if save_parsed {
                save_parsed_json(&services).await?;
            }

            // Save original JSON if requested
            if save_original {
                save_raw_json(&services_json, SERVICES_FULL_FILE).await?;
            }
        }
        Commands::Calendar => {
            // Load trash schedule from current directory
            let services = load_trash_services()?;

            // Generate calendar from the loaded services
            let calendar = calendar::generate_calendar(&services)?;

            // Save calendar
            let calendar_content = calendar.to_string();
            std::fs::write(&cli.output, calendar_content)
                .context("Failed to write calendar file")?;

            println!("Calendar saved to: {:?}", cli.output);
        }
    }

    Ok(())
}
