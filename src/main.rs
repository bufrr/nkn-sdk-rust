use nkn_sdk_rust::error::NcpError;
use nkn_sdk_rust::session::{NcpConfig, SendWith, Session};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use tokio::spawn;

pub struct TestSession {
    pub local_client_id: String,
    pub remote_client_id: String,
    pub session: Session,
}

impl TestSession {
    pub fn new(session: Session) -> Self {
        TestSession {
            local_client_id: "alice".to_string(),
            remote_client_id: "bob".to_string(),
            session,
        }
    }

    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, NcpError> {
        self.session.write(buf).await
    }

    pub async fn read(&mut self, buf: &mut [u8]) {
        self.session
            .receive_with(
                self.local_client_id.clone(),
                self.remote_client_id.clone(),
                buf,
            )
            .await
    }
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
        ncp_config,
        "".to_string(),
        "".to_string(),
        Vec::new(),
        Vec::new(),
        ss,
    );

    let mut test_session = TestSession::new(sess);

    test_session
        .write(b"hello")
        .await
        .expect("TODO: panic message");
}
