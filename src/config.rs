use std::fmt;
use std::net::SocketAddr;

pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
pub const DEFAULT_BODY_LIMIT_BYTES: usize = 15 * 1024 * 1024;
pub const DEFAULT_MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    BindAddr(String),
    BodyLimit(String),
    ThreadCount(String),
    DecompressLimit(String),
    Extractor(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BindAddr(raw) => write!(f, "invalid EXTRACT_BIND_ADDR: {raw:?}"),
            Self::BodyLimit(raw) => write!(f, "invalid EXTRACT_BODY_LIMIT_BYTES: {raw:?}"),
            Self::ThreadCount(raw) => write!(f, "invalid EXTRACT_NUM_THREADS: {raw:?}"),
            Self::DecompressLimit(raw) => {
                write!(f, "invalid EXTRACT_MAX_DECOMPRESSED_BYTES: {raw:?}")
            }
            Self::Extractor(raw) => write!(f, "invalid EXTRACT_EXTRACTOR: {raw:?}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extractor {
    Lean,
    Lopdf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub body_limit_bytes: usize,
    pub thread_count: usize,
    pub max_decompressed_bytes: usize,
    pub extractor: Extractor,
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

        let max_decompressed_bytes = match get_env("EXTRACT_MAX_DECOMPRESSED_BYTES") {
            Some(raw) => parse_positive(&raw, ConfigError::DecompressLimit)?,
            None => DEFAULT_MAX_DECOMPRESSED_BYTES,
        };

        let extractor = match get_env("EXTRACT_EXTRACTOR").as_deref() {
            Some("lean") | None => Extractor::Lean,
            Some("lopdf") => Extractor::Lopdf,
            Some(raw) => return Err(ConfigError::Extractor(raw.to_string())),
        };

        Ok(Self {
            bind_addr,
            body_limit_bytes,
            thread_count,
            max_decompressed_bytes,
            extractor,
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

    use super::{
        Config, ConfigError, DEFAULT_BIND_ADDR, DEFAULT_BODY_LIMIT_BYTES,
        DEFAULT_MAX_DECOMPRESSED_BYTES, Extractor,
    };

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
                max_decompressed_bytes: DEFAULT_MAX_DECOMPRESSED_BYTES,
                extractor: Extractor::Lean,
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
                max_decompressed_bytes: DEFAULT_MAX_DECOMPRESSED_BYTES,
                extractor: Extractor::Lean,
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
                max_decompressed_bytes: DEFAULT_MAX_DECOMPRESSED_BYTES,
                extractor: Extractor::Lean,
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

    #[test]
    fn applies_explicit_max_decompressed_bytes() {
        let config = Config::from(env(&[("EXTRACT_MAX_DECOMPRESSED_BYTES", "1048576")]));

        assert_eq!(
            config,
            Ok(Config {
                bind_addr: DEFAULT_BIND_ADDR.parse().unwrap(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                thread_count: super::default_thread_count(),
                max_decompressed_bytes: 1048576,
                extractor: Extractor::Lean,
            })
        );
    }

    #[test]
    fn rejects_invalid_max_decompressed_bytes() {
        let config = Config::from(env(&[("EXTRACT_MAX_DECOMPRESSED_BYTES", "huge")]));

        assert_eq!(
            config,
            Err(ConfigError::DecompressLimit("huge".to_string()))
        );
    }

    #[test]
    fn rejects_zero_max_decompressed_bytes() {
        let config = Config::from(env(&[("EXTRACT_MAX_DECOMPRESSED_BYTES", "0")]));

        assert_eq!(config, Err(ConfigError::DecompressLimit("0".to_string())));
    }

    #[test]
    fn selects_lopdf_extractor_for_rollback() {
        let config = Config::from(env(&[("EXTRACT_EXTRACTOR", "lopdf")])).unwrap();

        assert_eq!(config.extractor, Extractor::Lopdf);
    }

    #[test]
    fn rejects_unknown_extractor() {
        let config = Config::from(env(&[("EXTRACT_EXTRACTOR", "other")]));

        assert_eq!(config, Err(ConfigError::Extractor("other".to_string())));
    }

    /// Asignaciones activas de `.env.example`, tal como las leería un shell
    /// con `source`: las comentadas no definen nada.
    fn documented_env() -> Vec<(String, String)> {
        env_example_lines()
            .iter()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), value.trim().to_string()))
            .collect()
    }

    /// Todas las claves que `.env.example` menciona, activas o comentadas.
    fn documented_keys() -> Vec<String> {
        env_example_lines()
            .iter()
            .filter_map(|line| {
                line.strip_prefix("# ")
                    .unwrap_or(line)
                    .split_once('=')
                    .map(|(key, _)| key.to_string())
            })
            .collect()
    }

    fn env_example_lines() -> Vec<String> {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.env.example"))
            .expect("el repositorio debe traer .env.example");

        raw.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#') || line.starts_with("# "))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn env_example_documents_every_known_key() {
        let documented = documented_keys();

        for known in [
            "EXTRACT_BIND_ADDR",
            "EXTRACT_BODY_LIMIT_BYTES",
            "EXTRACT_NUM_THREADS",
            "EXTRACT_MAX_DECOMPRESSED_BYTES",
            "EXTRACT_EXTRACTOR",
        ] {
            assert!(
                documented.iter().any(|key| key == known),
                "`.env.example` no documenta {known}"
            );
        }
    }

    #[test]
    fn env_example_values_are_accepted_by_config() {
        let pairs = documented_env();
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, value)| value.clone())
        };

        // `cp .env.example .env` + `source` no debe dejar al servicio sin arrancar
        assert!(
            Config::from(get).is_ok(),
            "`.env.example` documenta valores que Config rechaza"
        );
    }

    #[test]
    fn env_example_bind_port_matches_dockerfile() {
        let documented = documented_env();
        let example_port = documented
            .iter()
            .find(|(key, _)| key == "EXTRACT_BIND_ADDR")
            .map(|(_, value)| value.as_str())
            .expect("EXTRACT_BIND_ADDR debe estar documentado");
        let example_port: u16 = example_port
            .rsplit_once(':')
            .expect("EXTRACT_BIND_ADDR debe ser host:puerto")
            .1
            .parse()
            .expect("el puerto documentado debe ser numérico");

        let dockerfile =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Dockerfile"))
                .expect("el repositorio debe traer Dockerfile");
        let image_bind = dockerfile
            .lines()
            .find_map(|line| line.trim().strip_prefix("ENV EXTRACT_BIND_ADDR="))
            .expect("el Dockerfile debe fijar EXTRACT_BIND_ADDR");
        let image_port: u16 = image_bind
            .rsplit_once(':')
            .expect("EXTRACT_BIND_ADDR debe ser host:puerto")
            .1
            .parse()
            .expect("el puerto de la imagen debe ser numérico");

        assert_eq!(
            example_port, image_port,
            "el puerto documentado no es el que escucha la imagen"
        );
    }
}
