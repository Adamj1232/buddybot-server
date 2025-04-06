use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use uuid::Uuid;
use serde::de::{self, Deserializer, Unexpected, Visitor};
use std::fmt;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    #[serde(deserialize_with = "validate_port")]
    pub port: u16,
    pub workers: u32,
    #[serde(default = "Uuid::new_v4")]
    pub instance_id: Uuid,
}

struct PortVisitor;

impl<'de> Visitor<'de> for PortVisitor {
    type Value = u16;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid port number (integer between 1 and 65535 or string representation)")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v > 0 && v <= 65535 {
            Ok(v as u16)
        } else {
            Err(E::invalid_value(Unexpected::Unsigned(v), &self))
        }
    }
    
    fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v > 0 {
            Ok(v)
        } else {
            Err(E::invalid_value(Unexpected::Unsigned(v as u64), &self))
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.parse::<u16>() {
            Ok(port) if port > 0 => Ok(port),
            _ => Err(E::invalid_value(Unexpected::Str(v), &self)),
        }
    }
}

fn validate_port<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(PortVisitor)
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
pub struct ScalingConfig {
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold: f32,
    #[serde(default = "default_memory_threshold")]
    pub memory_threshold: f32,
    #[serde(default = "default_connection_threshold")]
    pub connection_threshold: u64,
    #[serde(default = "default_scale_up_factor")]
    pub scale_up_factor: f32,
    #[serde(default = "default_scale_down_factor")]
    pub scale_down_factor: f32,
    #[serde(default = "default_cooldown_period")]
    pub cooldown_period: i64,
}

fn default_cpu_threshold() -> f32 { 70.0 }
fn default_memory_threshold() -> f32 { 80.0 }
fn default_connection_threshold() -> u64 { 1000 }
fn default_scale_up_factor() -> f32 { 1.5 }
fn default_scale_down_factor() -> f32 { 0.5 }
fn default_cooldown_period() -> i64 { 300 }

#[derive(Debug, Deserialize, Clone)]
pub struct CorsConfig {
    #[serde(default = "default_cors_enabled")]
    pub enabled: bool,
    #[serde(default = "default_cors_allow_any_origin")]
    pub allow_any_origin: bool,
    #[serde(default = "default_cors_max_age")]
    pub max_age: u32,
}

fn default_cors_enabled() -> bool { true }
fn default_cors_allow_any_origin() -> bool { false }
fn default_cors_max_age() -> u32 { 3600 }

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub environment: String,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub scaling: ScalingConfig,
    pub cors: CorsConfig,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            // Set defaults first (lowest priority)
            .set_default("environment", "development")?
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", "8080")?
            .set_default("server.workers", num_cpus::get() as u32)?
            .set_default("database.url", "postgres://postgres:postgres@localhost/buddybot")?
            .set_default("database.max_connections", 5)?
            .set_default("auth.jwt_secret", "development_secret")?
            .set_default("auth.token_expiry_hours", 24)?
            .set_default("scaling.cpu_threshold", 70.0)?
            .set_default("scaling.memory_threshold", 80.0)?
            .set_default("scaling.connection_threshold", 1000)?
            .set_default("scaling.scale_up_factor", 1.5)?
            .set_default("scaling.scale_down_factor", 0.5)?
            .set_default("scaling.cooldown_period", 300)?
            .set_default("cors.enabled", true)?
            .set_default("cors.allow_any_origin", false)?
            .set_default("cors.max_age", 3600)?
            
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
            .set_default("server.port", "8080")?
            .set_default("server.workers", num_cpus::get() as u32)?
            .set_default("database.url", "postgres://postgres:postgres@localhost/test")?
            .set_default("database.max_connections", 2)?
            .set_default("auth.jwt_secret", "test_secret")?
            .set_default("auth.token_expiry_hours", 1)?
            .set_default("scaling.cpu_threshold", 70.0)?
            .set_default("scaling.memory_threshold", 80.0)?
            .set_default("scaling.connection_threshold", 1000)?
            .set_default("scaling.scale_up_factor", 1.5)?
            .set_default("scaling.scale_down_factor", 0.5)?
            .set_default("scaling.cooldown_period", 300)?
            .set_default("cors.enabled", true)?
            .set_default("cors.allow_any_origin", false)?
            .set_default("cors.max_age", 3600)?
            
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
        env::remove_var("APP_SCALING__CPU_THRESHOLD");
        env::remove_var("APP_SCALING__MEMORY_THRESHOLD");
        env::remove_var("APP_SCALING__CONNECTION_THRESHOLD");
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
        assert_eq!(settings.scaling.cpu_threshold, 70.0);
        assert_eq!(settings.scaling.memory_threshold, 80.0);
        assert_eq!(settings.scaling.connection_threshold, 1000);
    }

    #[test]
    fn test_environment_override() {
        cleanup_env();
        
        // Set environment variables
        env::set_var("APP_SERVER__PORT", "9000");
        env::set_var("APP_DATABASE__URL", "postgres://test:test@localhost/test");
        env::set_var("APP_DATABASE__MAX_CONNECTIONS", "5");
        env::set_var("APP_AUTH__JWT_SECRET", "override_secret");
        env::set_var("APP_AUTH__TOKEN_EXPIRY_HOURS", "48");
        env::set_var("RUN_MODE", "test"); // Ensure test mode
        
        let settings = Settings::new().expect("Failed to load settings");
        
        // Verify overrides
        assert_eq!(settings.server.port, 9000, "Port override failed");
        assert_eq!(settings.database.url, "postgres://test:test@localhost/test", "Database URL override failed");
        assert_eq!(settings.database.max_connections, 5, "Max connections override failed");
        assert_eq!(settings.auth.jwt_secret, "override_secret", "JWT secret override failed");
        assert_eq!(settings.auth.token_expiry_hours, 48, "Token expiry override failed");
        
        cleanup_env();
    }

    #[test]
    fn test_invalid_port() {
        cleanup_env();
        env::set_var("RUN_MODE", "test"); // Ensure test mode
        
        // Test with invalid port string
        env::set_var("APP_SERVER__PORT", "invalid_port");
        let result = Settings::new();
        assert!(result.is_err(), "Expected error for invalid port string");
        
        // Test with out of range port
        env::set_var("APP_SERVER__PORT", "99999");
        let result = Settings::new();
        assert!(result.is_err(), "Expected error for out of range port");
        
        // Test with zero port
        env::set_var("APP_SERVER__PORT", "0");
        let result = Settings::new();
        assert!(result.is_err(), "Expected error for zero port");

        // Test with negative port (will fail string parsing)
        env::set_var("APP_SERVER__PORT", "-1");
        let result = Settings::new();
        assert!(result.is_err(), "Expected error for negative port");
        
        cleanup_env();
    }
}