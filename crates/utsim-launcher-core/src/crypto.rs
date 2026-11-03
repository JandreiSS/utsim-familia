//! Package encryption and per-tester key wrapping.
//!
//! **Package** (`.utpk`): the plaintext (a .zip of the program folder) is cut into chunks,
//! each sealed with XChaCha20-Poly1305 under a random 256-bit *content key* that is new for
//! every package. Nonces follow the STREAM construction (a random 19-byte prefix, a 32-bit
//! chunk counter and a "last chunk" flag), so chunks cannot be reordered, dropped or the file
//! truncated without detection, and a package of any size is processed in constant memory.
//!
//! ```text
//! header (32 bytes) = "UTSIMPK" 0x01 | chunk size (u32 BE) | nonce prefix (19) | 0x00
//! chunk i          = XChaCha20-Poly1305(key, prefix | i (u32 BE) | last?1:0, aad = header)
//! ```
//!
//! **Wrapping**: the content key is sealed once per tester allowed to see the package, with a
//! key derived from the tester's access code by Argon2id (random salt per wrap). The AEAD
//! associated data names the package (product, version, platform) and the tester id, so a
//! wrap cannot be moved to another package. Revoking a tester means not wrapping for them in
//! the next packages.

use std::io::{Read, Write};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::code::AccessCode;
use crate::{Error, Result};

const MAGIC: &[u8; 8] = b"UTSIMPK\x01";
/// Size of the package header.
pub const HEADER_LEN: usize = 32;
const PREFIX_LEN: usize = 19;
const TAG_LEN: usize = 16;
/// Default plaintext chunk size (1 MiB).
pub const DEFAULT_CHUNK: u32 = 1 << 20;
/// Name of the cipher written in the manifest.
pub const PACKAGE_CIPHER: &str = "xchacha20poly1305-stream-v1";
/// Name of the wrapping scheme written in the manifest.
pub const WRAP_SCHEME: &str = "argon2id-xchacha20poly1305-v1";

/// A 256-bit content key, wiped from memory on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ContentKey([u8; 32]);

impl ContentKey {
    pub fn generate() -> Result<Self> {
        let mut k = [0u8; 32];
        crate::random_bytes(&mut k)?;
        Ok(Self(k))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Base64 form, used only by the author's local key store.
    pub fn to_base64(&self) -> String {
        B64.encode(self.0)
    }

    pub fn from_base64(s: &str) -> Result<Self> {
        let mut v = B64
            .decode(s.trim())
            .map_err(|e| Error::Format(format!("chave de conteúdo em base64 inválida: {e}")))?;
        let arr: [u8; 32] = v
            .as_slice()
            .try_into()
            .map_err(|_| Error::Format("chave de conteúdo deve ter 32 bytes".into()))?;
        v.zeroize();
        Ok(Self(arr))
    }
}

impl std::fmt::Debug for ContentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContentKey(****)")
    }
}

fn cipher(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

fn chunk_nonce(prefix: &[u8], index: u32, last: bool) -> XNonce {
    let mut n = [0u8; 24];
    n[..PREFIX_LEN].copy_from_slice(prefix);
    n[PREFIX_LEN..PREFIX_LEN + 4].copy_from_slice(&index.to_be_bytes());
    n[23] = u8::from(last);
    XNonce::from(n)
}

/// Reads until `buf` is full or the input ends; returns how many bytes were read.
fn read_full(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Encrypts `input` into `output` as a package; returns the bytes written.
pub fn encrypt_stream(
    key: &ContentKey,
    mut input: impl Read,
    mut output: impl Write,
    chunk_size: u32,
) -> Result<u64> {
    if chunk_size == 0 {
        return Err(Error::Format("tamanho de bloco zero".into()));
    }
    let mut header = [0u8; HEADER_LEN];
    header[..8].copy_from_slice(MAGIC);
    header[8..12].copy_from_slice(&chunk_size.to_be_bytes());
    crate::random_bytes(&mut header[12..12 + PREFIX_LEN])?;
    output.write_all(&header)?;
    let prefix = header[12..12 + PREFIX_LEN].to_vec();
    let aead = cipher(key.as_bytes());

    let size = chunk_size as usize;
    let mut current = vec![0u8; size];
    let mut next = vec![0u8; size];
    let mut cur_len = read_full(&mut input, &mut current)?;
    let mut index: u32 = 0;
    let mut written = HEADER_LEN as u64;
    loop {
        let next_len = if cur_len == size {
            read_full(&mut input, &mut next)?
        } else {
            0
        };
        let last = next_len == 0;
        let sealed = aead
            .encrypt(
                &chunk_nonce(&prefix, index, last),
                Payload {
                    msg: &current[..cur_len],
                    aad: &header,
                },
            )
            .map_err(|_| Error::Format("falha ao cifrar".into()))?;
        output.write_all(&sealed)?;
        written += sealed.len() as u64;
        if last {
            break;
        }
        index = index
            .checked_add(1)
            .ok_or_else(|| Error::Format("pacote grande demais".into()))?;
        std::mem::swap(&mut current, &mut next);
        cur_len = next_len;
    }
    current.zeroize();
    next.zeroize();
    output.flush()?;
    Ok(written)
}

/// Decrypts a package from `input` into `output`; returns the plaintext bytes written.
///
/// Any change to the file (a flipped bit, a missing or reordered chunk, truncation) fails
/// with [`Error::Integrity`]. Output already written before the failure must be discarded
/// by the caller.
pub fn decrypt_stream(
    key: &ContentKey,
    mut input: impl Read,
    mut output: impl Write,
) -> Result<u64> {
    let mut header = [0u8; HEADER_LEN];
    if read_full(&mut input, &mut header)? != HEADER_LEN || &header[..8] != MAGIC {
        return Err(Error::Integrity(
            "não é um pacote do UTSim (cabeçalho)".into(),
        ));
    }
    let chunk_size = u32::from_be_bytes(header[8..12].try_into().unwrap_or_default());
    if chunk_size == 0 || chunk_size > 64 << 20 {
        return Err(Error::Integrity("tamanho de bloco inválido".into()));
    }
    let prefix = header[12..12 + PREFIX_LEN].to_vec();
    let aead = cipher(key.as_bytes());

    let size = chunk_size as usize + TAG_LEN;
    let mut current = vec![0u8; size];
    let mut next = vec![0u8; size];
    let mut cur_len = read_full(&mut input, &mut current)?;
    let mut index: u32 = 0;
    let mut written = 0u64;
    loop {
        if cur_len < TAG_LEN {
            return Err(Error::Integrity("pacote truncado".into()));
        }
        let next_len = if cur_len == size {
            read_full(&mut input, &mut next)?
        } else {
            0
        };
        let last = next_len == 0;
        let mut plain = aead
            .decrypt(
                &chunk_nonce(&prefix, index, last),
                Payload {
                    msg: &current[..cur_len],
                    aad: &header,
                },
            )
            .map_err(|_| {
                Error::Integrity(format!(
                    "o bloco {index} não confere (alterado ou truncado)"
                ))
            })?;
        output.write_all(&plain)?;
        written += plain.len() as u64;
        plain.zeroize();
        if last {
            break;
        }
        index = index
            .checked_add(1)
            .ok_or_else(|| Error::Integrity("pacote grande demais".into()))?;
        std::mem::swap(&mut current, &mut next);
        cur_len = next_len;
    }
    current.zeroize();
    next.zeroize();
    output.flush()?;
    Ok(written)
}

/// Argon2id cost parameters, stored with each wrap so they can grow over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argon2Params {
    /// Memory in KiB.
    pub m_kib: u32,
    /// Iterations.
    pub t: u32,
    /// Lanes.
    pub p: u32,
}

impl Argon2Params {
    /// 64 MiB, 3 passes, 1 lane: about half a second on an ordinary laptop.
    pub const DEFAULT: Self = Self {
        m_kib: 64 * 1024,
        t: 3,
        p: 1,
    };
    /// Cheap parameters for tests only.
    pub const TEST: Self = Self {
        m_kib: 64,
        t: 1,
        p: 1,
    };
}

/// The content key of one package, sealed for one tester.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    /// Public id of the tester's access code (first group).
    pub id: String,
    /// Argon2id salt, base64.
    pub salt: String,
    /// XChaCha20-Poly1305 nonce, base64.
    pub nonce: String,
    /// Sealed content key (32 bytes + 16-byte tag), base64.
    pub key: String,
    pub argon2: Argon2Params,
}

fn derive_kek(code: &AccessCode, salt: &[u8], params: Argon2Params) -> Result<[u8; 32]> {
    // Bounds keep a hostile manifest from asking for absurd memory (it is signed, but cheap
    // to check): 8 KiB to 1 GiB, 1 to 16 passes, 1 to 8 lanes.
    if !(8..=1 << 20).contains(&params.m_kib)
        || !(1..=16).contains(&params.t)
        || !(1..=8).contains(&params.p)
    {
        return Err(Error::Format(
            "parâmetros de Argon2id fora dos limites".into(),
        ));
    }
    let p = argon2::Params::new(params.m_kib, params.t, params.p, Some(32))
        .map_err(|e| Error::Format(format!("parâmetros de Argon2id: {e}")))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, p);
    let mut out = [0u8; 32];
    a.hash_password_into(code.secret_bytes(), salt, &mut out)
        .map_err(|e| Error::Format(format!("Argon2id: {e}")))?;
    Ok(out)
}

fn wrap_aad(context: &str, id: &str) -> Vec<u8> {
    format!("{WRAP_SCHEME}|{context}|{id}").into_bytes()
}

/// Seals `content` for the holder of `code`. `context` names the package
/// (see [`package_context`]).
pub fn wrap_key(
    content: &ContentKey,
    code: &AccessCode,
    context: &str,
    params: Argon2Params,
) -> Result<WrappedKey> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    crate::random_bytes(&mut salt)?;
    crate::random_bytes(&mut nonce)?;
    let mut kek = derive_kek(code, &salt, params)?;
    let sealed = cipher(&kek)
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: content.as_bytes(),
                aad: &wrap_aad(context, code.id()),
            },
        )
        .map_err(|_| Error::Format("falha ao embrulhar a chave".into()));
    kek.zeroize();
    Ok(WrappedKey {
        id: code.id().to_string(),
        salt: B64.encode(salt),
        nonce: B64.encode(nonce),
        key: B64.encode(sealed?),
        argon2: params,
    })
}

/// Recovers the content key with the tester's code; [`Error::NoAccess`] when the code is
/// not the one this key was wrapped for.
pub fn unwrap_key(wrapped: &WrappedKey, code: &AccessCode, context: &str) -> Result<ContentKey> {
    let decode = |s: &str, what: &str| {
        B64.decode(s)
            .map_err(|e| Error::Format(format!("{what} em base64 inválido: {e}")))
    };
    let salt = decode(&wrapped.salt, "sal")?;
    let nonce: [u8; 24] = decode(&wrapped.nonce, "nonce")?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Format("nonce deve ter 24 bytes".into()))?;
    let sealed = decode(&wrapped.key, "chave")?;
    let mut kek = derive_kek(code, &salt, wrapped.argon2)?;
    let opened = cipher(&kek).decrypt(
        &XNonce::from(nonce),
        Payload {
            msg: &sealed,
            aad: &wrap_aad(context, &wrapped.id),
        },
    );
    kek.zeroize();
    let mut opened = opened.map_err(|_| Error::NoAccess)?;
    let arr: [u8; 32] = opened
        .as_slice()
        .try_into()
        .map_err(|_| Error::Format("chave de conteúdo com tamanho errado".into()))?;
    opened.zeroize();
    Ok(ContentKey(arr))
}

/// Finds the wrap for `code` among a package's wraps and opens it.
pub fn unwrap_for(wraps: &[WrappedKey], code: &AccessCode, context: &str) -> Result<ContentKey> {
    let mut last = Error::NoAccess;
    for w in wraps.iter().filter(|w| w.id == code.id()) {
        match unwrap_key(w, code, context) {
            Ok(k) => return Ok(k),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The associated data that ties a wrap to one package.
pub fn package_context(product: &str, version: &str, platform: &str) -> String {
    format!("{product}|{version}|{platform}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(len: usize, chunk: u32) {
        let key = ContentKey::generate().unwrap();
        let data: Vec<u8> = (0..len).map(|i| (i * 7 % 251) as u8).collect();
        let mut sealed = Vec::new();
        let n = encrypt_stream(&key, &data[..], &mut sealed, chunk).unwrap();
        assert_eq!(n as usize, sealed.len());
        let mut plain = Vec::new();
        decrypt_stream(&key, &sealed[..], &mut plain).unwrap();
        assert_eq!(plain, data, "len {len} chunk {chunk}");
    }

    #[test]
    fn roundtrip_sizes() {
        for len in [0, 1, 15, 16, 17, 63, 64, 65, 128, 1000] {
            roundtrip(len, 16);
        }
        roundtrip(3 << 20, DEFAULT_CHUNK);
    }

    #[test]
    fn wrong_key_fails() {
        let key = ContentKey::generate().unwrap();
        let mut sealed = Vec::new();
        encrypt_stream(&key, &b"segredo"[..], &mut sealed, 16).unwrap();
        let other = ContentKey::generate().unwrap();
        let r = decrypt_stream(&other, &sealed[..], &mut Vec::new());
        assert!(matches!(r, Err(Error::Integrity(_))));
    }

    #[test]
    fn tampering_truncation_and_reordering_fail() {
        let key = ContentKey::generate().unwrap();
        let data = [42u8; 100];
        let mut sealed = Vec::new();
        encrypt_stream(&key, &data[..], &mut sealed, 16).unwrap();
        let chunk = 16 + TAG_LEN;

        let mut flipped = sealed.clone();
        flipped[HEADER_LEN + 3] ^= 1;
        assert!(decrypt_stream(&key, &flipped[..], &mut Vec::new()).is_err());

        // Cut exactly at a chunk boundary: the remaining last chunk is not marked "last".
        let cut = &sealed[..HEADER_LEN + 2 * chunk];
        assert!(matches!(
            decrypt_stream(&key, cut, &mut Vec::new()),
            Err(Error::Integrity(_))
        ));

        let mut swapped = sealed.clone();
        let (a, b) = (HEADER_LEN, HEADER_LEN + chunk);
        let first: Vec<u8> = swapped[a..b].to_vec();
        let second: Vec<u8> = swapped[b..b + chunk].to_vec();
        swapped[a..b].copy_from_slice(&second);
        swapped[b..b + chunk].copy_from_slice(&first);
        assert!(decrypt_stream(&key, &swapped[..], &mut Vec::new()).is_err());

        let mut header = sealed.clone();
        header[20] ^= 0x80; // nonce prefix
        assert!(decrypt_stream(&key, &header[..], &mut Vec::new()).is_err());
    }

    #[test]
    fn wrap_roundtrip_wrong_code_and_wrong_context() {
        let content = ContentKey::generate().unwrap();
        let code = AccessCode::generate().unwrap();
        let ctx = package_context("utsim", "0.2.0", "windows-x64");
        let w = wrap_key(&content, &code, &ctx, Argon2Params::TEST).unwrap();
        assert_eq!(w.id, code.id());
        let back = unwrap_key(&w, &code, &ctx).unwrap();
        assert_eq!(back.as_bytes(), content.as_bytes());

        let other = AccessCode::generate().unwrap();
        assert!(matches!(unwrap_key(&w, &other, &ctx), Err(Error::NoAccess)));
        assert!(matches!(
            unwrap_for(std::slice::from_ref(&w), &other, &ctx),
            Err(Error::NoAccess)
        ));
        let moved = package_context("utsim", "0.3.0", "windows-x64");
        assert!(matches!(
            unwrap_key(&w, &code, &moved),
            Err(Error::NoAccess)
        ));
    }

    #[test]
    fn absurd_argon2_params_are_refused() {
        let content = ContentKey::generate().unwrap();
        let code = AccessCode::generate().unwrap();
        let mut w = wrap_key(&content, &code, "x", Argon2Params::TEST).unwrap();
        w.argon2.m_kib = u32::MAX;
        assert!(matches!(unwrap_key(&w, &code, "x"), Err(Error::Format(_))));
    }
}
