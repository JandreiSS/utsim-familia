//! Per-tester access codes.
//!
//! A code reads `IDID-XXXX-XXXX-XXXX-XXXC`: 20 characters of Crockford base32 in five groups.
//! - The first group is the tester's **public id**: it appears in the manifest next to the
//!   content key wrapped for that tester, so the launcher runs Argon2id once instead of once
//!   per tester. It is random and carries no name.
//! - The next 15 characters are the **secret** (75 bits from the OS random source).
//! - The last character is a check digit (5 bits of SHA-256), so a typo is reported at once
//!   instead of as "no access".
//!
//! Input is forgiving: case, spaces and dashes are ignored, and `O`, `I`, `L` and `U` read
//! as `0`, `1`, `1` and `V` (Crockford's rules).

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Characters of the public id.
pub const ID_LEN: usize = 4;
const SECRET_LEN: usize = 15;
const TOTAL_LEN: usize = ID_LEN + SECRET_LEN + 1;

/// A parsed, normalized access code. The secret is wiped from memory on drop.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct AccessCode {
    /// The 20 normalized characters, without dashes.
    chars: String,
}

impl AccessCode {
    /// A new random code.
    pub fn generate() -> Result<Self> {
        let mut raw = [0u8; ID_LEN + SECRET_LEN];
        crate::random_bytes(&mut raw)?;
        let mut chars: String = raw
            .iter()
            .map(|b| ALPHABET[(b & 31) as usize] as char)
            .collect();
        raw.zeroize();
        chars.push(check_char(&chars));
        Ok(Self { chars })
    }

    /// Parses what a tester typed.
    pub fn parse(input: &str) -> Result<Self> {
        let mut chars = String::with_capacity(TOTAL_LEN);
        for c in input.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                'U' => 'V',
                c => c,
            };
            if !ALPHABET.contains(&(c as u8)) || !c.is_ascii() {
                return Err(Error::Code(format!(
                    "caractere '{c}' não faz parte de códigos"
                )));
            }
            chars.push(c);
        }
        if chars.len() != TOTAL_LEN {
            return Err(Error::Code(format!(
                "o código tem {TOTAL_LEN} caracteres (sem contar os traços), este tem {}",
                chars.len()
            )));
        }
        let (body, check) = chars.split_at(TOTAL_LEN - 1);
        if !check.starts_with(check_char(body)) {
            return Err(Error::Code(
                "o código tem um erro de digitação (o dígito verificador não confere)".into(),
            ));
        }
        Ok(Self { chars })
    }

    /// The public id (first group).
    pub fn id(&self) -> &str {
        &self.chars[..ID_LEN]
    }

    /// The bytes fed to Argon2id: the whole normalized code.
    pub fn secret_bytes(&self) -> &[u8] {
        self.chars.as_bytes()
    }

    /// The code in groups of four, as shown to the author and typed by the tester.
    pub fn formatted(&self) -> String {
        self.chars
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("-")
    }
}

impl fmt::Debug for AccessCode {
    // Never prints the secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AccessCode({}-****)", self.id())
    }
}

fn check_char(body: &str) -> char {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(body.as_bytes());
    ALPHABET[(digest[0] & 31) as usize] as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_parses_back() {
        let code = AccessCode::generate().unwrap();
        let shown = code.formatted();
        assert_eq!(shown.len(), 24);
        assert_eq!(AccessCode::parse(&shown).unwrap(), code);
        assert_eq!(AccessCode::parse(&shown.to_lowercase()).unwrap(), code);
        assert_eq!(code.id(), &shown[..4]);
    }

    #[test]
    fn crockford_aliases_and_spaces() {
        let code = AccessCode::generate().unwrap();
        let typed = code
            .formatted()
            .replace('0', "o")
            .replace('1', "l")
            .replace('-', " ");
        assert_eq!(AccessCode::parse(&typed).unwrap(), code);
    }

    #[test]
    fn typo_is_caught() {
        let body = "K7QD3M9XTR2AHH4PW8Z";
        let good = format!("{body}{}", check_char(body));
        assert!(AccessCode::parse(&good).is_ok());
        // A typo in the secret: the stored check digit no longer matches. Pick a
        // substitution that changes the check digit (31 of 32 do).
        let typo = ALPHABET
            .iter()
            .map(|&c| format!("K7QD3M{}XTR2AHH4PW8Z{}", c as char, check_char(body)))
            .find(|t| check_char(&t[..TOTAL_LEN - 1]) != check_char(body))
            .unwrap();
        assert!(matches!(AccessCode::parse(&typo), Err(Error::Code(_))));
    }

    #[test]
    fn wrong_length_and_bad_chars() {
        assert!(matches!(
            AccessCode::parse("ABCD-1234"),
            Err(Error::Code(_))
        ));
        assert!(matches!(
            AccessCode::parse("ABCD-1234-ABCD-1234-ABC!"),
            Err(Error::Code(_))
        ));
    }

    #[test]
    fn debug_hides_secret() {
        let code = AccessCode::generate().unwrap();
        let shown = format!("{code:?}");
        assert!(!shown.contains(&code.formatted()[5..]));
    }
}
