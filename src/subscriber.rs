use std::{collections::{HashMap, HashSet}, sync::Arc};

use tokio::sync::{mpsc, watch, Mutex};



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

pub struct SubscriberManager{
    exchange: String,
    url: String,
    channel: String,
    max_per_subscriber: usize,
    subscribers: Vec<Arc<Mutex<Subscriber>>>,
    output_tx: mpsc::Sender<Message>,
    output_rx: mpsc::Receiver<Message>,
    subscriptions: HashSet<String>,
    update_rx: watch::Receiver<HashMap<String, ReferentialData>>,
    local_refdata: HashMap<String, ReferentialData>,
    zmq_tx: mpsc::Sender<Vec<u8>>,
}

impl SubscriberManager {
    pub fn new(
        exchange: &str,
        url: &str,
        channel: &str,
        max_per_subscriber: usize,
        update_rx: watch::Receiver<HashMap<String, ReferentialData>>,
        zmq_tx: mpsc::Sender<Vec<u8>>,
    ) -> Self {
        let (output_tx, output_rx) = mpsc::channel(100);
        let subscribers = Vec::new();
        let subscriptions = HashSet::new();
        let local_refdata = HashMap::new();

        Self {
            exchange: exchange.to_string(),
            url: url.to_string(),
            channel: channel.to_string(),
            max_per_subscriber,
            subscribers,
            output_tx,
            output_rx,
            subscriptions,
            update_rx,
            local_refdata,
            zmq_tx,
        }
    }
}

pub struct Subscriber {
    exchange: String,
    url: String,
    channel: String,
    subscriptions: Arc<Mutex<HashSet<String>>>,
    update_tx: mpsc::Sender<HashSet<String>>,
    update_rx: mpsc::Receiver<HashSet<String>>,
    output_tx: mpsc::Sender<Message>,
}