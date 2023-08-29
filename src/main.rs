use nkn_sdk_rust::session::{NcpConfig, SendWith, Session};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use tokio::spawn;

pub struct TestSession {
    pub session: Session,
}

#[tokio::main]
async fn main() {
    let ncp_config = NcpConfig::default();

    spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:30080").unwrap();
        for stream in listener.incoming() {
            println!("new client!");
        }
    });

    let mut stream = TcpStream::connect("127.0.0.1:30080").unwrap();
    //let &mut stream_clone = stream.try_clone().unwrap();
    let send1 = move |local_client_id: String,
                      remote_client_id: String,
                      buf: &[u8],
                      timeout: Duration|
          -> Result<usize, std::io::Error> {
        stream.write_all(buf).unwrap();
        Ok(0)
    };
    let ss: SendWith = Box::new(send1);
    let sess = Session::new(
        ncp_config.clone(),
        "".to_string(),
        "".to_string(),
        Vec::new(),
        Vec::new(),
        ss,
    );

    let mut test_session = TestSession { session: sess };
}
