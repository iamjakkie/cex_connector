mod subscriber;
mod refdata;

use std::{collections::HashMap, env};

use subscriber::SubscriptionMeta;


#[tokio::main]
async fn main() {
    let mut referential_data_path = env::var("REF_DATA_PATH").unwrap_or_else(|_| {
        println!("REF_DATA_PATH not set, using default");
        "data".to_string()
    });

    let sources: HashMap<String, SubscriptionMeta> = HashMap::from(
        [("okx-perp".to_string(),
        SubscriptionMeta {
            exchange: "okx".to_string(),
            channels: vec![],
            ws_url: "".to_string(),
            rest_url: "".to_string(),
            max_symbols_per_sub: 200,
            refdata_path: format!("{}/{}", referential_data_path, "okx.parquet"),
        })]
    );
}
