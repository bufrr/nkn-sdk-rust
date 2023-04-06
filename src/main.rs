use nkn_sdk_rust::account::Account;
use nkn_sdk_rust::wallet::Wallet;

fn main() {
    let acc = Account::new().unwrap();
    let config = nkn_sdk_rust::wallet::WalletConfig::default();
    let w = Wallet::new(acc, config).unwrap();
    let wallet_str = w.to_json();
    println!("{}", wallet_str);

    //     let wallet_str = String::from(
    //         r###"{"Version":2,"IV":"9af0d1fc4fa01cad8a75b10ef9d6bcfc","MasterKey":"5acd549e229b28dda16d036ac602d0eff8330ec1f0d14f5b80574d43540766ed","SeedEncrypted":"122336e42053eb891b38c9202899174fd79a3c75b0937fd35add83a55dd66aa5","Address":"NKNafu2vquMeFfDXZMF3JMfFABcBei5gcdWg","Scrypt":{"N":32768,"R":8,"P":1,"Salt":"d920403b67c07ed3"}}
    // "###,
    //     );

    // let config2 = nkn_sdk_rust::wallet::WalletConfig::default();
    // println!("1234");
    // let w2 = Wallet::from_json(&wallet_str, config2).unwrap();
    // let wallet_str2 = w2.to_json();
    // println!("{wallet_str2}");
}
