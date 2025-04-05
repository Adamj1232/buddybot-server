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
            // Set defaults first (lowest priority)
            .set_default("environment", "development")?
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", num_cpus::get() as u32)?
            .set_default("database.url", "postgres://postgres:postgres@localhost/buddybot")?
            .set_default("database.max_connections", 5)?
            .set_default("auth.jwt_secret", "development_secret")?
            .set_default("auth.token_expiry_hours", 24)?
            
            // Add config files (medium priority)
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            
            // Add environment variables (highest priority)
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
            // Set defaults first (lowest priority)
            .set_default("environment", "test")?
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", num_cpus::get() as u32)?
            .set_default("database.url", "postgres://postgres:postgres@localhost/test")?
            .set_default("database.max_connections", 2)?
            .set_default("auth.jwt_secret", "test_secret")?
            .set_default("auth.token_expiry_hours", 1)?
            
            // Add environment variables (highest priority)
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
        env::remove_var("APP_DATABASE__MAX_CONNECTIONS");
        env::remove_var("APP_AUTH__JWT_SECRET");
        env::remove_var("APP_AUTH__TOKEN_EXPIRY_HOURS");
        env::remove_var("APP_ENVIRONMENT");
        env::remove_var("RUN_MODE");
    }

    #[test]
    fn test_settings_defaults() {
        cleanup_env();
        let settings = Settings::new_for_test().expect("Failed to load settings");
        assert_eq!(settings.environment, "test");
        assert_eq!(settings.server.host, "127.0.0.1");
        assert_eq!(settings.server.port, 8080);
        assert_eq!(settings.server.workers, num_cpus::get() as u32);
        assert_eq!(settings.database.url, "postgres://postgres:postgres@localhost/test");
        assert_eq!(settings.database.max_connections, 2);
    }

    #[test]
    fn test_environment_override() {
        cleanup_env();
        
        // Set environment variables
        env::set_var("APP_SERVER__PORT", "9000");
        env::set_var("APP_SERVER__WORKERS", "4");
        env::set_var("APP_DATABASE__URL", "postgres://test:test@localhost/test");
        env::set_var("APP_DATABASE__MAX_CONNECTIONS", "5");
        env::set_var("APP_AUTH__JWT_SECRET", "override_secret");
        env::set_var("APP_AUTH__TOKEN_EXPIRY_HOURS", "48");
        
        let settings = Settings::new().expect("Failed to load settings");
        
        // Verify overrides
        assert_eq!(settings.server.port, 9000, "Port override failed");
        assert_eq!(settings.server.workers, 4, "Workers override failed");
        assert_eq!(settings.database.url, "postgres://test:test@localhost/test", "Database URL override failed");
        assert_eq!(settings.database.max_connections, 5, "Max connections override failed");
        assert_eq!(settings.auth.jwt_secret, "override_secret", "JWT secret override failed");
        assert_eq!(settings.auth.token_expiry_hours, 48, "Token expiry override failed");
        
        cleanup_env();
    }

    #[test]
    fn test_invalid_port() {
        cleanup_env();
        
        env::set_var("APP_SERVER__PORT", "invalid_port");
        
        let result = Settings::new();
        assert!(result.is_err(), "Expected error for invalid port");
        
        if let Err(e) = result {
            let error_message = e.to_string().to_lowercase();
            assert!(
                error_message.contains("invalid") || 
                error_message.contains("error") || 
                error_message.contains("failed") ||
                error_message.contains("invalid value"),
                "Error message '{}' should indicate an invalid value",
                error_message
            );
        }
        
        cleanup_env();
    }
}