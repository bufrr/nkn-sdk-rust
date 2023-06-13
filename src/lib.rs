extern crate core;

pub mod account;
pub mod client;
pub mod constants;
pub mod crypto;
mod error;
pub mod message;
pub mod multiclient;
mod nanopay;
mod payload;
pub mod program;
mod rpc;
mod serialization;
mod sigchain;
pub mod signature;
pub mod tx;
pub mod utils;
pub mod wallet;

pub mod pb {
    pub mod transaction {
        include!(concat!(env!("OUT_DIR"), "/pb.transaction.rs"));
    }
    pub mod payloads {
        include!(concat!(env!("OUT_DIR"), "/pb.payloads.rs"));
    }
    pub mod node {
        include!(concat!(env!("OUT_DIR"), "/pb.node.rs"));
    }
    pub mod block {
        include!(concat!(env!("OUT_DIR"), "/pb.block.rs"));
    }
    pub mod sigchain {
        include!(concat!(env!("OUT_DIR"), "/pb.sigchain.rs"));
    }
    pub mod nodemessage {
        include!(concat!(env!("OUT_DIR"), "/pb.nodemessage.rs"));
    }
    pub mod clientmessage {
        include!(concat!(env!("OUT_DIR"), "/pb.clientmessage.rs"));
    }
}

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

//no_mangle 来告诉编译器不要破坏函数名，确保我们的函数名称被导入到 C 文件
#[no_mangle]
pub unsafe extern "C" fn rust_greeting(to: *const c_char) -> *mut c_char {
    let c_str = unsafe {
        assert!(!to.is_null());
        CStr::from_ptr(to)
    };
    let recipient = match c_str.to_str() {
        Err(_) => "there",
        Ok(string) => string,
    };
    CString::new("Hello ".to_owned() + recipient)
        .unwrap()
        .into_raw()
}

#[no_mangle]
#[deny(clippy::not_unsafe_ptr_arg_deref)]
pub unsafe extern "C" fn rust_greeting_free(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        CString::from_raw(s)
    };
}
