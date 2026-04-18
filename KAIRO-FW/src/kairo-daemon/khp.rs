use std::convert::TryInto;

pub const KHP_VERSION: u8 = 0x01;
pub const MSG_CHECK_LISTEN: u8 = 1;

#[derive(Debug, Clone, Copy)]
pub struct KhpHeader {
    pub version: u8,
    pub msg_type: u8,
    pub flags: u8,
    pub reserved: u8,
    pub msg_len: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CheckListenReq {
    pub pid: u32,
    pub port: u16,
    pub proto: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct CheckListenResp {
    pub decision: u8, // 0 deny, 1 allow
}

impl KhpHeader {
    pub fn decode(buf: &[u8]) -> Result<Self, String> {
        if buf.len() < 8 {
            return Err("header too short".to_string());
        }
        let msg_len = u32::from_be_bytes(buf[4..8].try_into().map_err(|_| "bad len".to_string())?);
        Ok(Self {
            version: buf[0],
            msg_type: buf[1],
            flags: buf[2],
            reserved: buf[3],
            msg_len,
        })
    }

    pub fn encode(&self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0] = self.version;
        out[1] = self.msg_type;
        out[2] = self.flags;
        out[3] = self.reserved;
        out[4..8].copy_from_slice(&self.msg_len.to_be_bytes());
        out
    }
}

pub fn decode_check_listen(payload: &[u8]) -> Result<CheckListenReq, String> {
    if payload.len() < 7 {
        return Err("check_listen payload too short".to_string());
    }
    let pid = u32::from_be_bytes(
        payload[0..4]
            .try_into()
            .map_err(|_| "pid parse".to_string())?,
    );
    let port = u16::from_be_bytes(
        payload[4..6]
            .try_into()
            .map_err(|_| "port parse".to_string())?,
    );
    let proto = payload[6];
    Ok(CheckListenReq { pid, port, proto })
}

pub fn encode_check_listen_resp(resp: CheckListenResp) -> Vec<u8> {
    let hdr = KhpHeader {
        version: KHP_VERSION,
        msg_type: MSG_CHECK_LISTEN,
        flags: 0,
        reserved: 0,
        msg_len: 1,
    };
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(&hdr.encode());
    out.push(resp.decision);
    out
}

pub fn proto_u8_to_str(v: u8) -> &'static str {
    match v {
        6 => "TCP",
        17 => "UDP",
        _ => "TCP",
    }
}
