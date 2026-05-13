use std::sync::Arc;

use reqwest::Client;
use tokio::sync::Semaphore;

use crate::config::Config;

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    pub http_client: Client,
    pub resize_semaphore: Arc<Semaphore>,
}
