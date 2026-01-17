use anyhow::{Context, Result};
use reqwest::{Client, cookie::Jar};
use reqwest::cookie::CookieStore;
use std::path::PathBuf;
use std::fs;
use std::sync::Arc;
use std::collections::HashSet;
use crate::config::Credentials;

/// Deduplicates cookies by removing duplicate cookie names (keeping the first occurrence)
fn deduplicate_cookies(cookie_str: &str) -> String {
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

    deduped_cookies.join("; ")
}

#[derive(Debug)]
pub struct PjhoyClient {
    pub config: Credentials,
    pub client: Client,
    pub cookie_jar: Arc<Jar>,
    pub config_dir: PathBuf,
}

impl PjhoyClient {
    pub fn new(config: Credentials, config_dir: PathBuf) -> Result<Self> {
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

    fn load_cookies(config_dir: &PathBuf) -> Result<Jar> {
        let cookie_path = config_dir.join("cookies.txt");

        if cookie_path.exists() {
            let cookie_data = fs::read_to_string(&cookie_path)
                .context("Failed to read cookies file")?;

            if cookie_data.trim().is_empty() {
                Ok(Jar::default())
            } else {
                let cookie_jar = Jar::default();
                let url = "https://extranet.pjhoy.fi/pirkka".parse().unwrap();

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

    pub fn save_cookies(&self) -> Result<()> {
        let cookie_path = self.config_dir.join("cookies.txt");
        let url = "https://extranet.pjhoy.fi/pirkka".parse().unwrap();
        let cookies = self.cookie_jar.cookies(&url);

        if let Some(cookie_header) = cookies {
            fs::write(&cookie_path, deduplicate_cookies(cookie_header.to_str()?))
                .context("Failed to save cookies")?;
        } else {
            // println!("Debug: No cookies to save");
            fs::write(&cookie_path, "")
                .context("Failed to save empty cookies file")?;
        }
        Ok(())
    }

    pub async fn login(&mut self) -> Result<()> {
        let login_url = "https://extranet.pjhoy.fi/pirkka/j_acegi_security_check?target=2";
        let base_url = "https://extranet.pjhoy.fi/pirkka";

        let params = [
            ("j_username", &self.config.username),
            ("j_password", &self.config.password),
            ("remember-me", &"false".to_string()),
        ];

        let _session_response = self.client
            .get(base_url)
            .send()
            .await
            .context("Failed to establish session")?;

        let response = self.client
            .post(login_url)
            .form(&params)
            .send()
            .await
            .context("Failed to send login request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Login failed: {}", response.status()));
        }

        let url = "https://extranet.pjhoy.fi/pirkka".parse().unwrap();

        for set_cookie_header in response.headers().get_all("set-cookie") {
            let set_cookie_str = set_cookie_header.to_str()?;
            self.cookie_jar.add_cookie_str(set_cookie_str, &url);
        }

        self.save_cookies()?;
        Ok(())
    }

    pub async fn fetch_trash_services(&self) -> Result<serde_json::Value> {
        let customer_numbers = &self.config.customer_numbers;
        let url = construct_api_url(&self.config.username, customer_numbers)?;

        let response = self.client
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
}

fn construct_api_url(username: &str, customer_numbers: &[String]) -> Result<String> {
    if customer_numbers.is_empty() {
        return Err(anyhow::anyhow!("No customer numbers configured"));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let cookie_str = "JSESSIONID=test123; JSESSIONIDVERSION=test456; JSESSIONIDVERSION=test789";
        let deduped = deduplicate_cookies(cookie_str);
        assert_eq!(deduped, "JSESSIONID=test123; JSESSIONIDVERSION=test456");
    }
}
