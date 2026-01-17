use anyhow::{Context, Result};
use config::{Config, File};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub customer_numbers: Vec<String>,
}

pub fn load_config(config_dir: &PathBuf) -> Result<Credentials> {
    let config_path = config_dir.join("config.toml");

    let settings = Config::builder()
        .add_source(File::from(config_path))
        .build()?;

    let credentials: Credentials = settings.try_deserialize()?;
    Ok(credentials)
}

pub fn get_project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("fi", "pjhoy", "pjhoy").context("Could not determine project directories")
}
