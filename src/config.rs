use std::fmt;
use std::net::SocketAddr;

pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
pub const DEFAULT_BODY_LIMIT_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    BindAddr(String),
    BodyLimit(String),
    ThreadCount(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BindAddr(raw) => write!(f, "invalid EXTRACT_BIND_ADDR: {raw:?}"),
            Self::BodyLimit(raw) => write!(f, "invalid EXTRACT_BODY_LIMIT_BYTES: {raw:?}"),
            Self::ThreadCount(raw) => write!(f, "invalid EXTRACT_NUM_THREADS: {raw:?}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub body_limit_bytes: usize,
    pub thread_count: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from(|key| std::env::var(key).ok())
    }

    fn from(get_env: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let bind_addr = match get_env("EXTRACT_BIND_ADDR") {
            Some(raw) => raw
                .parse::<SocketAddr>()
                .map_err(|_| ConfigError::BindAddr(raw))?,
            None => DEFAULT_BIND_ADDR
                .parse()
                .expect("default bind addr is valid"),
        };

        let body_limit_bytes = match get_env("EXTRACT_BODY_LIMIT_BYTES") {
            Some(raw) => parse_positive(&raw, ConfigError::BodyLimit)?,
            None => DEFAULT_BODY_LIMIT_BYTES,
        };

        let thread_count = match get_env("EXTRACT_NUM_THREADS") {
            Some(raw) => parse_positive(&raw, ConfigError::ThreadCount)?,
            None => default_thread_count(),
        };

        Ok(Self {
            bind_addr,
            body_limit_bytes,
            thread_count,
        })
    }
}

fn parse_positive(raw: &str, error: impl Fn(String) -> ConfigError) -> Result<usize, ConfigError> {
    let parsed = raw.parse::<usize>().map_err(|_| error(raw.to_string()))?;
    if parsed == 0 {
        return Err(error(raw.to_string()));
    }
    Ok(parsed)
}

fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{Config, ConfigError, DEFAULT_BIND_ADDR, DEFAULT_BODY_LIMIT_BYTES};

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        move |key| map.get(key).map(|value| value.to_string())
    }

    #[test]
    fn applies_explicit_values_from_environment() {
        let config = Config::from(env(&[
            ("EXTRACT_BIND_ADDR", "127.0.0.1:9090"),
            ("EXTRACT_BODY_LIMIT_BYTES", "4096"),
            ("EXTRACT_NUM_THREADS", "4"),
        ]));

        assert_eq!(
            config,
            Ok(Config {
                bind_addr: "127.0.0.1:9090".parse().unwrap(),
                body_limit_bytes: 4096,
                thread_count: 4,
            })
        );
    }

    #[test]
    fn falls_back_to_defaults_when_vars_unset() {
        let config = Config::from(env(&[]));

        assert_eq!(
            config,
            Ok(Config {
                bind_addr: DEFAULT_BIND_ADDR.parse().unwrap(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                thread_count: super::default_thread_count(),
            })
        );
    }

    #[test]
    fn falls_back_for_individual_missing_vars() {
        let config = Config::from(env(&[
            ("EXTRACT_BIND_ADDR", "0.0.0.0:3000"),
            ("EXTRACT_BODY_LIMIT_BYTES", "1048576"),
        ]));

        assert_eq!(
            config,
            Ok(Config {
                bind_addr: "0.0.0.0:3000".parse().unwrap(),
                body_limit_bytes: 1048576,
                thread_count: super::default_thread_count(),
            })
        );
    }

    #[test]
    fn rejects_invalid_thread_count() {
        let config = Config::from(env(&[("EXTRACT_NUM_THREADS", "many")]));

        assert_eq!(config, Err(ConfigError::ThreadCount("many".to_string())));
    }

    #[test]
    fn rejects_zero_thread_count() {
        let config = Config::from(env(&[("EXTRACT_NUM_THREADS", "0")]));

        assert_eq!(config, Err(ConfigError::ThreadCount("0".to_string())));
    }

    #[test]
    fn rejects_invalid_body_limit() {
        let config = Config::from(env(&[("EXTRACT_BODY_LIMIT_BYTES", "unlimited")]));

        assert_eq!(config, Err(ConfigError::BodyLimit("unlimited".to_string())));
    }

    #[test]
    fn rejects_zero_body_limit() {
        let config = Config::from(env(&[("EXTRACT_BODY_LIMIT_BYTES", "0")]));

        assert_eq!(config, Err(ConfigError::BodyLimit("0".to_string())));
    }

    #[test]
    fn rejects_invalid_bind_addr() {
        let config = Config::from(env(&[("EXTRACT_BIND_ADDR", "not-an-address")]));

        assert_eq!(
            config,
            Err(ConfigError::BindAddr("not-an-address".to_string()))
        );
    }
}
