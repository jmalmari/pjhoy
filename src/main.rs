use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{Config, File};
use directories::ProjectDirs;
use reqwest::{Client, cookie::Jar};
use reqwest::cookie::CookieStore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::sync::Arc;
use std::collections::HashSet;

/// Deduplicates cookies by removing duplicate cookie names (keeping the first occurrence)
///
/// # Arguments
///
/// * `cookie_str` - Semicolon-separated cookie string (e.g., "JSESSIONID=abc; JSESSIONIDVERSION=123")
///
/// # Returns
///
/// Deduplicated cookie string with the same format
fn deduplicate_cookies(cookie_str: &str) -> String {
    let mut seen_cookies = HashSet::new();
    let mut deduped_cookies = Vec::new();

    for cookie_part in cookie_str.split(';') {
        let cookie_part = cookie_part.trim();
        if !cookie_part.is_empty() {
            // Extract just the cookie name (before the = sign)
            let cookie_name = cookie_part.split('=').next().unwrap_or("");

            // Only add if we haven't seen this cookie name before
            if !seen_cookies.contains(cookie_name) {
                seen_cookies.insert(cookie_name.to_string());
                deduped_cookies.push(cookie_part.to_string());
            }
        }
    }

    deduped_cookies.join("; ")
}


use chrono::NaiveDate;
use ics::{ICalendar, Event};
use ics::properties::{Summary, DtStart};
use chrono::Utc;

const SERVICES_FILE: &str = "services.json";
const SERVICES_FULL_FILE: &str = "services_full.json";
const ICS_FILE: &str = "pjhoy.ics";

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_url_construction() -> Result<()> {
        // Test case 1: Standard username format
        let username = "02-2891001-00";
        let customer_numbers = vec!["00".to_string(), "01".to_string(), "02".to_string()];

        let url = construct_api_url(username, &customer_numbers)?;

        // Verify URL contains expected customer numbers
        assert!(url.contains("customerNumbers%5B%5D=02-2891001-00"));
        assert!(url.contains("customerNumbers%5B%5D=02-2891001-01"));
        assert!(url.contains("customerNumbers%5B%5D=02-2891001-02"));

        // Test case 2: Different username format
        let username2 = "02-2030045-99";
        let customer_numbers2 = vec!["99".to_string(), "98".to_string()];

        let url2 = construct_api_url(username2, &customer_numbers2)?;

        assert!(url2.contains("customerNumbers%5B%5D=02-2030045-99"));
        assert!(url2.contains("customerNumbers%5B%5D=02-2030045-98"));

        Ok(())
    }

    #[test]
    fn test_cookie_deduplication() {
        use std::collections::HashSet;

        // Test the cookie deduplication logic
        let cookie_str = "JSESSIONID=test123; JSESSIONIDVERSION=test456; JSESSIONIDVERSION=test789";

        let mut seen_cookies = HashSet::new();
        let mut deduped_cookies = Vec::new();

        for cookie_part in cookie_str.split(';') {
            let cookie_part = cookie_part.trim();
            if !cookie_part.is_empty() {
                let cookie_name = cookie_part.split('=').next().unwrap_or("");
                if !seen_cookies.contains(cookie_name) {
                    seen_cookies.insert(cookie_name.to_string());
                    deduped_cookies.push(cookie_part.to_string());
                }
            }
        }

        let deduped_cookie_str = deduped_cookies.join("; ");

        // Verify that duplicates were removed
        assert_eq!(deduped_cookie_str, "JSESSIONID=test123; JSESSIONIDVERSION=test456");

        // Verify that we have exactly 2 cookies (no duplicates)
        assert_eq!(deduped_cookies.len(), 2);

        // Verify that JSESSIONIDVERSION appears only once
        assert_eq!(seen_cookies.len(), 2);
    }

    #[test]
    fn test_event_creation_with_timestamp() -> Result<()> {
        // Create a sample trash service
        let service = TrashService {
            ASTNextDate: Some("2023-12-25".to_string()),
            ASTNimi: "Test Trash Pickup".to_string(),
            ASTAsnro: "12345".to_string(),
            ASTPos: 1,
            ASTTyyppi: Some(1),
        };

        // Generate the event
        let event = generate_calendar_event(&service)?;

        // Convert event to string
        let event_str = event.to_string();
        println!("Generated event:\n{}", event_str);

        // Parse the event into a dictionary-like structure (HashMap)
        // This allows us to test individual properties more easily
        use std::collections::HashMap;

        let mut properties = HashMap::new();

        // Parse each line of the event (skip BEGIN/END lines and empty lines)
        for line in event_str.lines() {
            let line = line.trim();
            if line.starts_with("BEGIN:") || line.starts_with("END:") || line.is_empty() {
                continue;
            }

            // Split each line into NAME:VALUE pairs
            if let Some((name, value)) = line.split_once(':') {
                // For properties that can appear multiple times (like DTSTAMP),
                // we'll store them as a vector
                properties.entry(name.to_string())
                    .or_insert_with(Vec::new)
                    .push(value.to_string());
            }
        }

        // Now we can test individual properties more precisely

        // Test UID
        assert_eq!(properties.get("UID"), Some(&vec!["pjhoy_12345_1_1_2023-12-25".to_string()]));

        // Test DTSTART (should remain unchanged)
        assert_eq!(properties.get("DTSTART"), Some(&vec!["20231225".to_string()]));

        // Test SUMMARY
        assert_eq!(properties.get("SUMMARY"), Some(&vec!["Trash pickup: Test Trash Pickup".to_string()]));

        // Test DTSTAMP - should have at least one entry with current timestamp
        if let Some(dtstamps) = properties.get("DTSTAMP") {
            assert!(!dtstamps.is_empty(), "DTSTAMP should have at least one entry");

            // At least one DTSTAMP should contain the 'T' character (indicating it has time component)
            assert!(dtstamps.iter().all(|s| s.contains('T')), "DTSTAMP must have time component");

            println!("DTSTAMP values found: {:?}", dtstamps);
        } else {
            panic!("DTSTAMP property not found in event");
        }

        Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(name = "pjhoy")]
#[command(about = "Pirkanmaan Jätehuolto Oy utility", long_about = None)]
struct Cli {
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
        #[arg(long = "save-original-json", short = 'o')]
        save_original: bool,
    },
    /// Generate ICS calendar from current data
    Calendar,
}

#[derive(Debug, Serialize, Deserialize)]
struct Credentials {
    username: String,
    password: String,
    customer_numbers: Vec<String>,
}

// Struct to match the actual API response structure
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]  // API uses camelCase field names
struct TrashService {
    ASTNextDate: Option<String>,  // Actual field name from API, can be null
    ASTNimi: String,              // Service name
    ASTAsnro: String,             // Customer number for uniqueness
    ASTPos: i32,                  // Position for uniqueness
    ASTTyyppi: Option<i32>,       // Service type ID
    // Other fields from the JSON response
}

#[derive(Debug)]
struct AppState {
    config: Credentials,
    client: Client,
    cookie_jar: Arc<Jar>,
    config_dir: PathBuf,
}

impl AppState {
    fn new() -> Result<Self> {
        let proj_dirs = ProjectDirs::from("fi", "pjhoy", "pjhoy")
            .context("Could not determine project directories")?;

        let config_dir = proj_dirs.config_dir().to_path_buf();
        std::fs::create_dir_all(&config_dir)
            .context("Could not create config directory")?;

        let config = Self::load_config(&config_dir)?;
        let cookie_jar = std::sync::Arc::new(Self::load_cookies(&config_dir)?);

        let client = Client::builder()
            .cookie_provider(cookie_jar.clone())
            .build()?;

        Ok(Self {
            config,
            client,
            cookie_jar,
            config_dir,
        })
    }

    fn load_config(config_dir: &PathBuf) -> Result<Credentials> {
        let config_path = config_dir.join("config.toml");

        let settings = Config::builder()
            .add_source(File::from(config_path))
            .build()?;

        let credentials: Credentials = settings.try_deserialize()?;
        Ok(credentials)
    }

    fn load_cookies(config_dir: &PathBuf) -> Result<Jar> {
        let cookie_path = config_dir.join("cookies.txt");

        if cookie_path.exists() {
            let cookie_data = fs::read_to_string(&cookie_path)
                .context("Failed to read cookies file")?;

            // Try to deserialize the cookies
            if cookie_data.trim().is_empty() {
                // Empty file, create new jar
                Ok(Jar::default())
            } else {
                // Create a new jar and add the saved cookies
                let cookie_jar = Jar::default();

                // Parse the cookie string and add each cookie individually
                // This handles multiple cookies separated by semicolons
                let url = "https://extranet.pjhoy.fi/pirkka".parse().unwrap();

                // Split by semicolon and add each cookie separately
                for cookie_str in cookie_data.split(';') {
                    let cookie_str = cookie_str.trim();
                    if !cookie_str.is_empty() {
                        cookie_jar.add_cookie_str(cookie_str, &url);
                    }
                }

                Ok(cookie_jar)
            }
        } else {
            Ok(Jar::default())
        }
    }

    fn save_cookies(&self) -> Result<()> {
        let cookie_path = self.config_dir.join("cookies.txt");

        // Save all cookies by iterating through them individually
        // The cookies() method might not return all cookies, so we'll use a different approach
        let url = "https://extranet.pjhoy.fi/pirkka".parse().unwrap();

        // Get all cookies as a string
        let cookies = self.cookie_jar.cookies(&url);

        if let Some(cookie_header) = cookies {
            // Save all cookies, not just the first one
            // The cookie header should contain all cookies separated by semicolons
            fs::write(&cookie_path, deduplicate_cookies(cookie_header.to_str()?))
                .context("Failed to save cookies")?;
        } else {
            // No cookies to save, but create an empty marker file
            println!("Debug: No cookies to save");
            fs::write(&cookie_path, "")
                .context("Failed to save empty cookies file")?;
        }

        Ok(())
    }

    fn has_cookies(&self) -> bool {
        // Check if we have any cookies in the jar
        let cookies = self.cookie_jar.cookies(&"https://extranet.pjhoy.fi/pirkka".parse().unwrap());
        cookies.is_some() && !cookies.unwrap().is_empty()
    }
}

async fn login(state: &mut AppState) -> Result<()> {
    let login_url = "https://extranet.pjhoy.fi/pirkka/j_acegi_security_check?target=2";
    let base_url = "https://extranet.pjhoy.fi/pirkka";

    let params = [
        ("j_username", &state.config.username),
        ("j_password", &state.config.password),
        ("remember-me", &"false".to_string()),
    ];

    // First, visit the base URL to establish a session and get JSESSIONID

    let _session_response = state.client
        .get(base_url)
        .send()
        .await
        .context("Failed to establish session")?;






    // Now proceed with the actual login
    let response = state.client
        .post(login_url)
        .form(&params)
        .send()
        .await
        .context("Failed to send login request")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Login failed: {}", response.status()));
    }

    // Login successful - cookies have been added to the jar

    // Use the login URL which includes the /pirkka path
    let url = "https://extranet.pjhoy.fi/pirkka".parse().unwrap();

    // Handle multiple Set-Cookie headers properly
    // HTTP responses can have multiple Set-Cookie headers, not just one



    for set_cookie_header in response.headers().get_all("set-cookie") {
        let set_cookie_str = set_cookie_header.to_str()?;


        // Each Set-Cookie header contains one cookie with its attributes
        // Add the entire cookie string (including attributes like Path, Secure, etc.)
        // This will update existing cookies or add new ones
        state.cookie_jar.add_cookie_str(set_cookie_str, &url);


    }



    // Cookies have been added to the jar successfully

    // Save cookies after successful login
    state.save_cookies()?;

    // Check if we have cookies (this uses the cookie_jar field to suppress warnings)
    let _has_cookies = state.has_cookies();

    Ok(())
}

/// Constructs the API URL for fetching trash schedule
///
/// # Arguments
///
/// * `username` - User's username in format "xx-yyyyyyy-zz"
/// * `customer_numbers` - List of customer number suffixes (e.g., ["00", "01"])
///
/// # Returns
///
/// Constructed URL string
///
/// # Errors
///
/// Returns error if username format is invalid or no customer numbers provided
fn construct_api_url(username: &str, customer_numbers: &[String]) -> Result<String> {
    if customer_numbers.is_empty() {
        return Err(anyhow::anyhow!("No customer numbers configured"));
    }

    // Extract the prefix from the username (format: xx-yyyyyyy-zz)
    let username_parts: Vec<&str> = username.split('-').collect();
    if username_parts.len() < 2 {
        return Err(anyhow::anyhow!("Invalid username format. Expected format: xx-yyyyyyy-zz"));
    }

    Ok(format!(
        "https://extranet.pjhoy.fi/pirkka/secure/get_services_by_customer_numbers.do?{}",
        customer_numbers.iter()
        .map(|cn| format!("customerNumbers%5B%5D={}-{}-{}",username_parts[0], username_parts[1], cn))
            .collect::<Vec<_>>()
            .join("&")
    ))
}

async fn fetch_trash_services(state: &AppState) -> Result<serde_json::Value> {
    // Use customer numbers from configuration
    let customer_numbers = &state.config.customer_numbers;

    let url = construct_api_url(&state.config.username, customer_numbers)?;

    let response = state.client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch trash schedule")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch schedule: {}", response.status()));
    }

    let json_response: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse JSON response")?;

    Ok(json_response)
}

fn generate_calendar_event(service: &TrashService) -> Result<Event<'_>> {
    // Skip services without a next pickup date (like rentals)
    let Some(next_date) = &service.ASTNextDate else {
        return Err(anyhow::anyhow!("Service has no next pickup date"));
    };

    // For all-day events, we use date-only format (YYYY-MM-DD)
    // All-day events should have DTEND as the day after the event
    let dstamp = NaiveDate::parse_from_str(next_date, "%Y-%m-%d").context("Failed to parse date")?;

    // Create a unique UID using ASTAsnro, ASTTyyppi, ASTPos, and ASTNextDate
    // Using underscores as separators to avoid ambiguity with dashes in ASTAsnro
    // Use ASTTyyppi if available, otherwise use a default value
    let service_type_id = service.ASTTyyppi.unwrap_or(0);

    let uid = format!("pjhoy_{}_{}_{}_{}",
                     service.ASTAsnro,
                     service_type_id,
                     service.ASTPos,
                     next_date);

    let event_date_str = dstamp.format("%Y%m%d").to_string();

    let mut event = Event::new(uid, Utc::now().format("%Y%m%dT%H%M%SZ").to_string());

    // Alternatively, the creation date could be done using
    // ASTLastModDate and ASTLastModTime.

    // // Add the start date as an all-day event (date-only format)
    event.push(DtStart::new(event_date_str));

    // Add the summary/description using ASTNimi
    event.push(Summary::new(format!("Trash pickup: {}", service.ASTNimi)));

    Ok(event)
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

async fn generate_calendar(services: &[TrashService]) -> Result<ICalendar<'_>> {
    let mut calendar = ICalendar::new("2.0", "-//pjhoy//trash calendar//EN");

    for service in services {
        // Skip services without a next pickup date (like rentals)
        if let Ok(event) = generate_calendar_event(service) {
            calendar.add_event(event);
        }
    }

    Ok(calendar)
}

/// Save the parsed services JSON to the schedule file in the current directory
async fn save_parsed_json(services: &[TrashService]) -> Result<()> {
    let json_string = serde_json::to_string_pretty(services)
        .context("Failed to serialize parsed services to JSON")?;

    fs::write(SERVICES_FILE, json_string)
        .context(format!("Failed to write JSON to {SERVICES_FILE}"))?;

    println!("Parsed services JSON saved to: {SERVICES_FILE}");

    Ok(())
}

/// Save the raw JSON response to a file in the current directory
async fn save_raw_json(raw_json: &serde_json::Value, filename: &str) -> Result<()> {
    let json_string = serde_json::to_string_pretty(raw_json)
        .context("Failed to serialize raw JSON to string")?;

    fs::write(filename, json_string)
        .context(format!("Failed to write JSON to {}", filename))?;

    println!("Original raw JSON data saved to: {}", filename);

    Ok(())
}



#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut state = AppState::new()?;

    match cli.command {
        Commands::Login => {
            login(&mut state).await?;
        }
        Commands::Fetch { save_parsed, save_original } => {
            let services_json = fetch_trash_services(&state).await?;
            let services: Vec<TrashService> = serde_json::from_value(services_json.clone())?;

            println!("Fetched {} trash services", services.len());

            let calendar = generate_calendar(&services).await?;

            // Save calendar file
            let calendar_content = calendar.to_string();
            std::fs::write(ICS_FILE, calendar_content)
                .context("Failed to write calendar file")?;
            println!("Calendar saved to: {}", ICS_FILE);

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
            let calendar = generate_calendar(&services).await?;

            // Save calendar
            let calendar_content = calendar.to_string();
            std::fs::write(ICS_FILE, calendar_content)
                .context("Failed to write calendar file")?;

            println!("Calendar saved to: {}", ICS_FILE);
        }
    }

    Ok(())
}
