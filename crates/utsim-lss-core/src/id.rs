//! Identifiers: UUID version 7 (RFC 9562), ordered by creation time and generated without a
//! server. This crate does no I/O, so the caller supplies the clock and the random bits
//! ([`Id::v7`]); `utsim-lss-store` does that with the system clock and the OS generator.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A UUID (in practice always version 7). Byte order is time order, so `Ord` sorts by
/// creation instant first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Id([u8; 16]);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "identificador inválido '{0}' (esperado um UUID como 0192f3a4-5b6c-7d8e-9f01-23456789abcd)"
)]
pub struct IdError(pub String);

impl Id {
    /// The nil UUID (all zeros).
    pub const NIL: Id = Id([0; 16]);

    /// A version 7 UUID from the Unix time in milliseconds (48 bits are kept) and 10 random
    /// bytes (74 of their bits are used; the rest is the version and variant).
    pub fn v7(unix_ms: u64, random: [u8; 10]) -> Id {
        let mut b = [0u8; 16];
        let ms = unix_ms & 0xFFFF_FFFF_FFFF;
        b[..6].copy_from_slice(&ms.to_be_bytes()[2..]);
        b[6] = 0x70 | (random[0] & 0x0F);
        b[7] = random[1];
        b[8] = 0x80 | (random[2] & 0x3F);
        b[9..].copy_from_slice(&random[3..]);
        Id(b)
    }

    pub const fn from_bytes(bytes: [u8; 16]) -> Id {
        Id(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The UUID version (7 for ids made by [`Id::v7`]).
    pub fn version(&self) -> u8 {
        self.0[6] >> 4
    }

    /// The creation instant of a version 7 id, Unix milliseconds.
    pub fn unix_ms(&self) -> u64 {
        let mut ms = [0u8; 8];
        ms[2..].copy_from_slice(&self.0[..6]);
        u64::from_be_bytes(ms)
    }

    pub fn is_nil(&self) -> bool {
        self.0 == [0; 16]
    }

    /// The last 8 hex digits (the random part): enough to tell ids apart on screen.
    pub fn short(&self) -> String {
        let s = self.to_string();
        s[s.len() - 8..].to_string()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({self})")
    }
}

impl FromStr for Id {
    type Err = IdError;

    /// Accepts the hyphenated form (36 characters), any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || IdError(s.chars().take(60).collect());
        let bytes = s.as_bytes();
        if bytes.len() != 36 {
            return Err(err());
        }
        let mut out = [0u8; 16];
        let mut n = 0;
        let mut i = 0;
        while i < 36 {
            if matches!(i, 8 | 13 | 18 | 23) {
                if bytes[i] != b'-' {
                    return Err(err());
                }
                i += 1;
                continue;
            }
            let hi = hex_digit(bytes[i]).ok_or_else(err)?;
            let lo = hex_digit(bytes[i + 1]).ok_or_else(err)?;
            out[n] = (hi << 4) | lo;
            n += 1;
            i += 2;
        }
        Ok(Id(out))
    }
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = std::borrow::Cow::<str>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v7_layout_and_round_trip() {
        let id = Id::v7(0x0192_F3A4_5B6C, [0xFF; 10]);
        assert_eq!(id.version(), 7);
        assert_eq!(id.as_bytes()[8] >> 6, 0b10, "variant RFC 9562");
        assert_eq!(id.unix_ms(), 0x0192_F3A4_5B6C);
        let text = id.to_string();
        assert_eq!(text.len(), 36);
        assert!(text.starts_with("0192f3a4-5b6c-7"));
        assert_eq!(text.parse::<Id>().unwrap(), id);
        assert_eq!(text.to_uppercase().parse::<Id>().unwrap(), id);
    }

    #[test]
    fn order_follows_time() {
        let a = Id::v7(1_000, [0xFF; 10]);
        let b = Id::v7(1_001, [0x00; 10]);
        assert!(a < b);
    }

    #[test]
    fn rejects_malformed() {
        for bad in [
            "",
            "0192f3a4-5b6c-7d8e-9f01-23456789abc",
            "0192f3a4x5b6c-7d8e-9f01-23456789abcd",
            "0192f3a4-5b6c-7d8e-9f01-23456789abcg",
            "0192f3a45b6c7d8e9f0123456789abcd0000",
        ] {
            assert!(bad.parse::<Id>().is_err(), "{bad}");
        }
    }

    #[test]
    fn serde_as_string() {
        let id = Id::v7(42, [7; 10]);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<Id>(&json).unwrap(), id);
        assert!(serde_json::from_str::<Id>("\"nope\"").is_err());
        assert_eq!(id.short().len(), 8);
        assert!(Id::NIL.is_nil());
    }
}
