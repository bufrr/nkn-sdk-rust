use futures_util::SinkExt;
use nkn_sdk_rust::account::Account;
use nkn_sdk_rust::client::{Client, ClientConfig};
use nkn_sdk_rust::message::MessageConfig;
use nkn_sdk_rust::multiclient::MultiClient;
use std::io;
use std::str;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let account = Account::new().unwrap();
    let client_config = ClientConfig::default();
    let mut multi_client =
        match MultiClient::new(account, String::from("rust"), 2, false, &client_config) {
            Ok(m) => m,
            Err(e) => {
                println!("error: {e}");
                return Ok(());
            }
        };
    let to = "ccbb5686f06d23fe1c6706d03d79a3880a26295b571d2d5e3a5e78d46b907c97";
    let _ = multi_client.wait_for_connect().unwrap();
    //sleep(Duration::from_secs(10)).await;
    multi_client
        .send_data(
            &[to],
            "hello rust with multiclient".as_bytes(),
            MessageConfig::default(),
        )
        .await;
    // let account = Account::new().unwrap();
    // let client_config = ClientConfig::default();
    // let mut client = Client::new(account, None, client_config).unwrap();
    // let to = "ccbb5686f06d23fe1c6706d03d79a3880a26295b571d2d5e3a5e78d46b907c97";
    // let _ = client.wait_for_connect().unwrap();
    // let _ = client
    //     .send_data(&[to], "hello rust".as_bytes(), MessageConfig::default())
    //     .await;
    // let msg = client.wait_for_message().unwrap();
    // println!(
    //     "msg received: {:?}",
    //     str::from_utf8(msg.data.as_slice()).unwrap()
    // );
    // let _ = client
    //     .send_data(&[to], "hello nkn".as_bytes(), MessageConfig::default())
    //     .await;
    // let msg = client.wait_for_message().unwrap();
    // println!(
    //     "msg received: {:?}",
    //     str::from_utf8(msg.data.as_slice()).unwrap()
    // );
    // sleep(Duration::from_secs(10000)).await;
    // println!("exit");
    sleep(Duration::from_secs(10)).await;
    multi_client
        .send_data(
            &[to],
            "hello rust with multiclient 2222".as_bytes(),
            MessageConfig::default(),
        )
        .await;

    sleep(Duration::from_secs(10)).await;

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
