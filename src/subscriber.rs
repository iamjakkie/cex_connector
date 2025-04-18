

#[derive(Clone, Copy)]
pub enum DataSource {
    WebSocket,
    Rest
}

pub struct SubscriptionMeta {
    pub exchange: String,
    pub channels: Vec<String>,
    pub ws_url: String,
    pub rest_url: String,
    pub max_symbols_per_sub: usize,
    pub refdata_path: String,
}

pub struct SubscriptionManager {
    exchange: String,
    url: String,
    channel: String,
    
}