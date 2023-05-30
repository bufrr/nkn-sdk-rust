use nkn_sdk_rust::account::Account;
use nkn_sdk_rust::client::{Client, ClientConfig};
use nkn_sdk_rust::message::MessageConfig;
use std::io;
use std::str;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let account = Account::new().unwrap();
    let client_config = ClientConfig::default();
    let mut client = Client::new(account, None, client_config).unwrap();
    let to = "";
    let _ = client.wait_for_connect().unwrap();
    let _ = client
        .send_data(&[to], "hello rust".as_bytes(), MessageConfig::default())
        .await;
    let msg = client.wait_for_message().unwrap();
    println!(
        "msg received: {:?}",
        str::from_utf8(msg.data.as_slice()).unwrap()
    );
    let _ = client
        .send_data(&[to], "hello nkn".as_bytes(), MessageConfig::default())
        .await;
    let msg = client.wait_for_message().unwrap();
    println!(
        "msg received: {:?}",
        str::from_utf8(msg.data.as_slice()).unwrap()
    );
    sleep(Duration::from_secs(10000)).await;
    println!("exit");
    Ok(())
}

// fn test() {
//     let acc = Account::new().unwrap();
//     let config = nkn_sdk_rust::wallet::WalletConfig::default();
//     let w = Wallet::new(acc, config).unwrap();
//
//     let mut n = 0;
//     let random_bytes = rand::thread_rng().gen::<[u8; UINT160SIZE]>();
//     let tx = Transaction::new_transfer_asset(
//         &random_bytes,
//         &random_bytes,
//         234141234,
//         12311317278,
//         78978798,
//     );
//     let data = tx.encode();
//     let begin = std::time::SystemTime::now();
//     while n < 100 {
//         let _signature = w.sign(data.as_slice());
//
//         n += 1;
//     }
//     let finish = std::time::SystemTime::now();
//     let duration = finish.duration_since(begin).unwrap();
//     println!("duration: {:?}", duration);
// }
