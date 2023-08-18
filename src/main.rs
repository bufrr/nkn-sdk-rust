use nkn_sdk_rust::session::{NcpConfig, SendWith, Session};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let ncp_config = NcpConfig::default();
    let sess = Session::new(
        ncp_config,
        "".to_string(),
        "".to_string(),
        Vec::new(),
        Vec::new(),
        send,
    );
}

fn send(
    local_client_id: String,
    remote_client_id: String,
    buf: &Vec<u8>,
    timeout: Duration,
) -> Result<(), String> {
    Ok(())
}
