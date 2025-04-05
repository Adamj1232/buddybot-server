use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub token_expiry_hours: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub environment: String,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            // Start with default values
            .set_default("environment", "development")?
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", num_cpus::get() as i64)?
            .set_default("database.url", "postgres://postgres:postgres@localhost/buddybot")?
            .set_default("database.max_connections", 5)?
            .set_default("auth.jwt_secret", "development_secret")?
            .set_default("auth.token_expiry_hours", 24)?
            
            // Add in settings from the config file if it exists
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            
            // Add in settings from environment variables (with prefix "APP_")
            // E.g., `APP_SERVER__PORT=5001` would set `Settings.server.port`
            .add_source(
                Environment::with_prefix("app")
                    .separator("__")
                    .try_parsing(true)
            )
            .build()?;

        s.try_deserialize()
    }

    #[cfg(test)]
    pub fn new_for_test() -> Result<Self, ConfigError> {
        Config::builder()
            .set_default("environment", "test")?
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", num_cpus::get() as i64)?
            .set_default("database.url", "postgres://postgres:postgres@localhost/test")?
            .set_default("database.max_connections", 2)?
            .set_default("auth.jwt_secret", "test_secret")?
            .set_default("auth.token_expiry_hours", 1)?
            .add_source(
                Environment::with_prefix("app")
                    .separator("__")
                    .try_parsing(true)
            )
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn cleanup_env() {
        env::remove_var("APP_SERVER__PORT");
        env::remove_var("APP_DATABASE__URL");
        env::remove_var("APP_SERVER__WORKERS");
        env::remove_var("APP_AUTH__JWT_SECRET");
        env::remove_var("APP_AUTH__TOKEN_EXPIRY_HOURS");
    }

    #[test]
    fn test_settings_defaults() {
        cleanup_env();
        let settings = Settings::new_for_test().expect("Failed to load settings");
        assert_eq!(settings.environment, "test");
        assert_eq!(settings.server.host, "127.0.0.1");
        assert_eq!(settings.server.port, 8080);
        assert_eq!(settings.server.workers as usize, num_cpus::get());
        assert_eq!(settings.database.url, "postgres://postgres:postgres@localhost/test");
        assert_eq!(settings.database.max_connections, 2);
    }

    #[test]
    fn test_environment_override() {
        cleanup_env();
        
        // Set environment variables for all required fields
        env::set_var("APP_ENVIRONMENT", "test");
        env::set_var("APP_SERVER__HOST", "127.0.0.1");
        env::set_var("APP_SERVER__PORT", "9000");
        env::set_var("APP_SERVER__WORKERS", "2");
        env::set_var("APP_DATABASE__URL", "postgres://test:test@localhost/test");
        env::set_var("APP_DATABASE__MAX_CONNECTIONS", "5");
        env::set_var("APP_AUTH__JWT_SECRET", "override_secret");
        env::set_var("APP_AUTH__TOKEN_EXPIRY_HOURS", "48");
        
        // Create config directly from environment
        let config = Config::builder()
            // Set default values
            .set_default("environment", "test").unwrap()
            .set_default("server.host", "127.0.0.1").unwrap()
            .set_default("server.port", 8080).unwrap()
            .set_default("server.workers", 2).unwrap()
            .set_default("database.url", "postgres://postgres:postgres@localhost/test").unwrap()
            .set_default("database.max_connections", 2).unwrap()
            .set_default("auth.jwt_secret", "test_secret").unwrap()
            .set_default("auth.token_expiry_hours", 1).unwrap()
            // Add environment variables last to override defaults
            .add_source(
                Environment::with_prefix("app")
                    .separator("__")
                    .try_parsing(true)
            )
            .build()
            .expect("Failed to build config")
            .try_deserialize::<Settings>()
            .expect("Failed to deserialize settings");
        
        // Verify overrides
        assert_eq!(config.environment, "test");
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.workers, 2);
        assert_eq!(config.database.url, "postgres://test:test@localhost/test");
        assert_eq!(config.database.max_connections, 5);
        assert_eq!(config.auth.jwt_secret, "override_secret");
        assert_eq!(config.auth.token_expiry_hours, 48);
        
        cleanup_env();
    }

    #[test]
    fn test_invalid_port() {
        cleanup_env();
        
        // Set environment variables for all required fields
        env::set_var("APP_ENVIRONMENT", "test");
        env::set_var("APP_SERVER__HOST", "127.0.0.1");
        env::set_var("APP_SERVER__PORT", "invalid");
        env::set_var("APP_SERVER__WORKERS", "2");
        env::set_var("APP_DATABASE__URL", "postgres://test:test@localhost/test");
        env::set_var("APP_DATABASE__MAX_CONNECTIONS", "5");
        env::set_var("APP_AUTH__JWT_SECRET", "test_secret");
        env::set_var("APP_AUTH__TOKEN_EXPIRY_HOURS", "24");
        
        // Create config directly from environment
        let result = Config::builder()
            // Set default values
            .set_default("environment", "test").unwrap()
            .set_default("server.host", "127.0.0.1").unwrap()
            .set_default("server.port", 8080).unwrap()
            .set_default("server.workers", 2).unwrap()
            .set_default("database.url", "postgres://postgres:postgres@localhost/test").unwrap()
            .set_default("database.max_connections", 2).unwrap()
            .set_default("auth.jwt_secret", "test_secret").unwrap()
            .set_default("auth.token_expiry_hours", 1).unwrap()
            // Add environment variables last to override defaults
            .add_source(
                Environment::with_prefix("app")
                    .separator("__")
                    .try_parsing(true)
            )
            .build()
            .and_then(|config| config.try_deserialize::<Settings>());
        
        assert!(result.is_err(), "Expected error for invalid port");
        
        if let Err(e) = result {
            let error_message = e.to_string();
            assert!(
                error_message.contains("invalid digit found in string") || 
                error_message.contains("invalid value"),
                "Unexpected error: {}",
                error_message
            );
        }
        
        cleanup_env();
    }
}