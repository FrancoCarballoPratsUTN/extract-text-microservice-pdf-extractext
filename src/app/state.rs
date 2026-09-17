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
