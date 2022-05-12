use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub token: Token,
    pub global: GlobalConfig,
    pub influx: Option<InfluxConfig>,
    #[cfg(feature = "sqlite")]
    pub sqlite: Option<SqliteConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalConfig {
    pub default_vehicle: Option<String>,
    pub default_vehicle_id: Option<u64>,
    pub logspec: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_ts: u64,
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
#[cfg(feature = "sqlite")]
pub struct SqliteConfig {
    pub file: String
}

#[cfg(test)]
mod tests {
    use toml;
    use toml::ser::Error;
    use tesla::OAuthToken;
    use super::*;

    #[test]
    fn test_global_config_serialize() {
        let config = Config {
            token: Token {
                access_token: "access_token".to_string(),
                refresh_token: "refresh_token".to_string(),
                expires_ts: 0,
            },
            global: GlobalConfig {
                default_vehicle: None,
                default_vehicle_id: None,
                logspec: Some("info".to_string()),
            },
            influx: None,
            #[cfg(feature = "sqlite")]
            sqlite: None,
        };

        match toml::to_string(&config) {
            Ok(str) => {
                assert!(str.len() > 0);
                println!("{}", str);
            }
            Err(e) => {
                assert!(false, format!("{:?}", e));
            }
        };
    }
}