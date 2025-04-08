use std::collections::HashMap;


#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let sources: HashMap<String, SubscriptionMeta> = HashMap::from(
        "okx-perp".to_string(),
        SubscriptionMeta {
            
        }
    )
}
