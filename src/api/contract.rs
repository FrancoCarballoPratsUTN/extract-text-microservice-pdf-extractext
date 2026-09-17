use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: &'static str,
}
