use bytes::{Buf, BufMut, BytesMut};
use std::str;

pub fn write_var_u64(bytes: &mut Vec<u8>, val: u64) {
    let mut buf = BytesMut::with_capacity(9);

    if val < 0xFD {
        buf.put_u8(val as u8);
        buf.truncate(1);
    } else if val <= 0xFFFF {
        buf.put_u8(0xFD);
        buf.put_u16(val as u16);
        buf.truncate(3);
    } else if val <= 0xFFFFFFFF {
        buf.put_u8(0xFE);
        buf.put_u32(val as u32);
        buf.truncate(5);
    } else {
        buf.put_u8(0xFF);
        buf.put_u64(val);
    }

    bytes.extend_from_slice(&buf);
}

pub fn read_var_u64(bytes: &mut Vec<u8>) -> u64 {
    let size = read_u8(bytes);

    if size == 0xFD {
        read_u16(bytes) as u64
    } else if size == 0xFE {
        read_u32(bytes) as u64
    } else if size == 0xFF {
        read_u64(bytes)
    } else {
        read_u8(bytes) as u64
    }
}

pub fn write_var_bytes(bytes: &mut Vec<u8>, val: &[u8]) {
    write_var_u64(bytes, val.len() as u64);
    bytes.extend_from_slice(val);
}

pub fn read_var_bytes(bytes: &mut Vec<u8>) -> Vec<u8> {
    let len = read_var_u64(bytes) as usize;
    let value = bytes.drain(..len).collect();
    value
}

pub fn write_var_str(bytes: &mut Vec<u8>, val: &str) {
    write_var_u64(bytes, val.len() as u64);
    bytes.extend_from_slice(val.as_bytes());
}

pub fn read_var_str(bytes: &mut Vec<u8>) -> String {
    let value = read_var_bytes(bytes);
    str::from_utf8(&value).unwrap().into()
}

pub fn write_u8(bytes: &mut Vec<u8>, val: u8) {
    bytes.push(val);
}

pub fn read_u8(bytes: &mut Vec<u8>) -> u8 {
    bytes.drain(0..1).next().unwrap()
}

pub fn write_u16(bytes: &mut Vec<u8>, val: u16) {
    let mut buf = BytesMut::with_capacity(2);
    buf.put_u16(val);
    bytes.extend_from_slice(&buf);
}

pub fn read_u16(bytes: &mut Vec<u8>) -> u16 {
    let mut buf = BytesMut::with_capacity(2);
    buf.put(bytes.drain(0..2).as_slice());
    buf.get_u16()
}

pub fn write_u32(bytes: &mut Vec<u8>, val: u32) {
    let mut buf = BytesMut::with_capacity(4);
    buf.put_u32(val);
    bytes.extend_from_slice(&buf);
}

pub fn read_u32(bytes: &mut Vec<u8>) -> u32 {
    let mut buf = BytesMut::with_capacity(4);
    buf.put(bytes.drain(0..4).as_slice());
    buf.get_u32()
}

pub fn write_u64(bytes: &mut Vec<u8>, val: u64) {
    let mut buf = BytesMut::with_capacity(8);
    buf.put_u64(val);
    bytes.extend_from_slice(&buf);
}

pub fn read_u64(bytes: &mut Vec<u8>) -> u64 {
    let mut buf = BytesMut::with_capacity(8);
    buf.put(bytes.drain(0..8).as_slice());
    buf.get_u64()
}

pub fn write_bool(bytes: &mut Vec<u8>, val: bool) {
    write_u8(bytes, val as u8);
}

pub fn read_bool(bytes: &mut Vec<u8>) -> bool {
    read_u8(bytes) != 0
}
