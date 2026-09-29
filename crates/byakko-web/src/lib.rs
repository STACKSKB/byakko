//! Optional browser boundary. Report IDs and asynchronous transport belong to
//! the WebHID adapter; editor and workflow policy belong to byakko-core.

mod executor;
mod session;
pub use executor::BrowserOperation;
pub use session::BrowserSession;

use byakko_protocol::nia87::{lighting, protocol};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy)]
enum Read {
    Identity,
    Profile,
    Lighting,
}

impl Read {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "identity" => Ok(Self::Identity),
            "profile" => Ok(Self::Profile),
            "lighting" => Ok(Self::Lighting),
            _ => Err("Unsupported read; only identity, profile and lighting are available".into()),
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::Identity => 0x80,
            Self::Profile => 0x85,
            Self::Lighting => lighting::LED_READ_COMMAND,
        }
    }

    fn request(self) -> Vec<u8> {
        match self {
            Self::Lighting => lighting::read_request().to_vec(),
            _ => protocol::read_request(self.opcode(), 0, 0).to_vec(),
        }
    }

    fn decode(self, payload: &[u8]) -> Result<Value, String> {
        if payload.len() != 64 || payload[0] != self.opcode() {
            return Err("Unexpected feature response length or opcode".into());
        }
        match self {
            Self::Lighting => {
                let observed = lighting::Lighting::decode(payload)?;
                Ok(json!({
                    "raw": observed.raw(),
                    "effectId": observed.effect_id(),
                    "effectName": observed.effect().map(|effect| effect.name),
                    "setting": observed.recognized_setting(),
                }))
            }
            // Retain identity/profile bytes without guessing undocumented fields.
            _ => Ok(json!({ "raw": payload })),
        }
    }
}

#[wasm_bindgen]
pub fn read_request(name: &str) -> Result<Vec<u8>, JsValue> {
    Read::parse(name)
        .map(Read::request)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn decode_reply(name: &str, payload: &[u8]) -> Result<String, JsValue> {
    Read::parse(name)
        .and_then(|read| read.decode(payload))
        .map(|value| value.to_string())
        .map_err(|error| JsValue::from_str(&error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_only_exposes_known_reads() {
        for (name, opcode) in [("identity", 0x80), ("profile", 0x85), ("lighting", 0x87)] {
            let request = Read::parse(name).unwrap().request();
            assert_eq!(request.len(), 64);
            assert_eq!(request[0], opcode);
            assert_eq!(request[7], 0xff - opcode);
            assert!(request[8..].iter().all(|byte| *byte == 0));
        }
        for unsupported in ["write", "0x07", "reset", "keymap", ""] {
            assert!(Read::parse(unsupported).is_err());
        }
    }

    #[test]
    fn malformed_and_wrong_replies_are_rejected() {
        for read in [Read::Identity, Read::Profile, Read::Lighting] {
            assert!(read.decode(&[]).is_err());
            assert!(read.decode(&[0; 63]).is_err());
            assert!(read.decode(&[0; 65]).is_err());
            assert!(read.decode(&[0; 64]).is_err());
        }
    }

    #[test]
    fn white_is_semantic_but_raw_bytes_and_unknown_values_survive() {
        let mut reply = [0; 64];
        reply[..8].copy_from_slice(&[0x87, 1, 0, 4, 7, 250, 255, 250]);
        reply[63] = 0xa5;
        let result = Read::Lighting.decode(&reply).unwrap();
        assert_eq!(result["setting"]["rgb"], json!([255, 255, 255]));
        assert_eq!(result["raw"], json!(reply.as_slice()));
        reply[1] = 0xfe;
        let unknown = Read::Lighting.decode(&reply).unwrap();
        assert!(unknown["setting"].is_null());
        assert_eq!(unknown["raw"][1], 0xfe);
    }
}
