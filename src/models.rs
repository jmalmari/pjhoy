use serde::{Deserialize, Serialize};

// Struct to match the actual API response structure
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)] // API uses camelCase field names
pub struct TrashService {
    pub ASTNextDate: Option<String>, // Actual field name from API, can be null
    pub ASTNimi: String,             // Service name
    pub ASTAsnro: String,            // Customer number for uniqueness
    pub ASTPos: i32,                 // Position for uniqueness
    pub ASTTyyppi: Option<i32>,      // Service type ID
    pub tariff: Option<Tariff>,      // Tariff information including productgroup
    pub ASTHinta: Option<f64>,       // Cost, excluding taxes
    pub ASTVali: String,             // Interval in weeks
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct Tariff {
    pub productgroup: Option<String>, // Product group identifier
    pub name: Option<String>,         // Tariff name
                                      // Other tariff fields
}
