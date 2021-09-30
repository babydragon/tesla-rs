use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub global: GlobalConfig,
    pub influx: Option<InfluxConfig>,
    pub sqlite: Option<SqliteConfig>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub api_token: String,
    pub default_vehicle: Option<String>,
    pub default_vehicle_id: Option<u64>,
    pub logspec: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InfluxConfig {
    pub url: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub interval: Option<u64>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SqliteConfig {
    pub file: String
}