mod calendar;
mod client;
mod config;
mod models;

use crate::client::PjhoyClient;
use crate::config::load_config;
use crate::models::TrashService;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

const SERVICES_FILE: &str = "services.json";
const SERVICES_FULL_FILE: &str = "services_full.json";

#[derive(Parser, Debug)]
#[command(name = "pjhoy")]
#[command(about = "Pirkanmaan Jätehuolto Oy utility", long_about = None)]
struct Cli {
    /// Output ICS calendar file path
    #[arg(long, short)]
    output: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Login to PJHOY extranet and save session cookies
    Login,
    /// Fetch trash schedule and update calendar
    Fetch {
        /// Save parsed services JSON to data directory
        #[arg(long = "save-json", short = 'j')]
        save_parsed: bool,

        /// Save original raw JSON response to data directory
        #[arg(long = "save-original-json", short = 'r')]
        save_original: bool,
    },
    /// Generate ICS calendar from current data
    Calendar,
}

/// Load trash schedule from trash_schedule.json file in data directory
fn load_trash_services(data_dir: &Path) -> Result<Vec<TrashService>> {
    let file_path = data_dir.join(SERVICES_FILE);
    if !file_path.exists() {
        return Err(anyhow::anyhow!(
            "{} not found in data directory",
            SERVICES_FILE
        ));
    }

    let schedule_data =
        std::fs::read_to_string(&file_path).context(format!("Failed to read {:?}", file_path))?;

    let services: Vec<TrashService> =
        serde_json::from_str(&schedule_data).context(format!("Failed to parse {:?}", file_path))?;

    Ok(services)
}

/// Save the parsed services JSON to the schedule file in the data directory
async fn save_parsed_json(services: &[TrashService], data_dir: &Path) -> Result<()> {
    let file_path = data_dir.join(SERVICES_FILE);
    let json_string = serde_json::to_string_pretty(services)
        .context("Failed to serialize parsed services to JSON")?;

    std::fs::write(&file_path, json_string)
        .context(format!("Failed to write JSON to {:?}", file_path))?;

    println!("Parsed services JSON saved to: {:?}", file_path);

    Ok(())
}

/// Save the raw JSON response to a file in the data directory
async fn save_raw_json(
    raw_json: &serde_json::Value,
    filename: &str,
    data_dir: &Path,
) -> Result<()> {
    let file_path = data_dir.join(filename);
    let json_string =
        serde_json::to_string_pretty(raw_json).context("Failed to serialize raw JSON to string")?;

    std::fs::write(&file_path, json_string)
        .context(format!("Failed to write JSON to {:?}", file_path))?;

    println!("Original raw JSON data saved to: {:?}", file_path);

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
    let mut client = PjhoyClient::new(config, data_dir.clone())?;

    // Determine output path for ICS file
    let output_path = cli.output.unwrap_or_else(|| data_dir.join("pjhoy.ics"));

    match cli.command {
        Commands::Login => {
            client.login().await?;
            println!("Login successful and cookies saved.");
        }
        Commands::Fetch {
            save_parsed,
            save_original,
        } => {
            let services_json = client.fetch_trash_services().await?;
            let services: Vec<TrashService> = serde_json::from_value(services_json.clone())?;

            println!("Fetched {} trash services", services.len());

            let calendar = calendar::generate_calendar(&services)?;

            // Save calendar file
            let calendar_content = calendar.to_string();
            std::fs::write(&output_path, calendar_content)
                .context("Failed to write calendar file")?;
            println!("Calendar saved to: {:?}", output_path);

            // Save parsed JSON if requested
            if save_parsed {
                save_parsed_json(&services, &data_dir).await?;
            }

            // Save original JSON if requested
            if save_original {
                save_raw_json(&services_json, SERVICES_FULL_FILE, &data_dir).await?;
            }
        }
        Commands::Calendar => {
            // Load trash schedule from data directory
            let services = load_trash_services(&data_dir)?;

            // Generate calendar from the loaded services
            let calendar = calendar::generate_calendar(&services)?;

            // Save calendar
            let calendar_content = calendar.to_string();
            std::fs::write(&output_path, calendar_content)
                .context("Failed to write calendar file")?;

            println!("Calendar saved to: {:?}", output_path);
        }
    }

    Ok(())
}
