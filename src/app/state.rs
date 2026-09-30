use std::sync::Arc;

use rayon::ThreadPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: Arc<ThreadPool>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(config.thread_count)
                .build()
                .expect("rayon thread pool with a valid thread count is buildable"),
        );
        Self { config, pool }
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::config::{Config, Extractor};

    fn test_config() -> Config {
        Config {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            body_limit_bytes: 1024,
            thread_count: 2,
            max_decompressed_bytes: 1024 * 1024,
            extractor: Extractor::Lean,
        }
    }

    #[test]
    fn builds_pool_with_the_configured_thread_count() {
        let state = AppState::new(test_config());

        assert_eq!(state.pool.current_num_threads(), 2);
    }
}
