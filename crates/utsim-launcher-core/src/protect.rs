//! The tester's access code at rest: on Windows it is kept in `estado.json` encrypted with the
//! Windows data protection (DPAPI, current-user scope), so a copy of the file taken to another
//! account or computer does not reveal it. Elsewhere it stays as typed (no such service).
//!
//! The stored text is `dpapi:<base64>` for a protected value and the plain code otherwise, so
//! files written by older launchers (plain code) still load and are protected on the next save.

/// Prefix of a protected value in `estado.json`.
pub const PREFIX: &str = "dpapi:";

/// Text to store for `secret`: protected on Windows, as is elsewhere (or if DPAPI fails).
pub fn protect(secret: &str) -> String {
    #[cfg(windows)]
    {
        use base64::Engine as _;
        if let Some(blob) = dpapi::protect(secret.as_bytes()) {
            return format!(
                "{PREFIX}{}",
                base64::engine::general_purpose::STANDARD.encode(blob)
            );
        }
    }
    secret.to_string()
}

/// The secret back from stored text. A plain value is returned as is; a protected one that
/// cannot be opened (another user or computer, damaged file) gives `None`.
pub fn unprotect(stored: &str) -> Option<String> {
    let Some(b64) = stored.strip_prefix(PREFIX) else {
        return Some(stored.to_string());
    };
    #[cfg(windows)]
    {
        use base64::Engine as _;
        let blob = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .ok()?;
        let plain = dpapi::unprotect(&blob)?;
        String::from_utf8(plain).ok()
    }
    #[cfg(not(windows))]
    {
        let _ = b64;
        None
    }
}

/// Whether `stored` is a protected value.
pub fn is_protected(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;
    use std::ptr;

    #[repr(C)]
    struct DataBlob {
        len: u32,
        data: *mut u8,
    }

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
    /// Extra entropy: a blob made for something else does not open here.
    const ENTROPY: &[u8] = b"UTSim launcher: codigo de acesso";

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }

    fn blob(bytes: &[u8]) -> DataBlob {
        DataBlob {
            len: bytes.len() as u32,
            data: bytes.as_ptr().cast_mut(),
        }
    }

    /// Copies the output blob and frees it (DPAPI allocates it with LocalAlloc).
    fn take(out: DataBlob) -> Vec<u8> {
        if out.data.is_null() {
            return Vec::new();
        }
        // SAFETY: DPAPI returned `len` valid bytes at `data`, freed right after the copy.
        let v = unsafe { std::slice::from_raw_parts(out.data, out.len as usize) }.to_vec();
        // SAFETY: `data` was allocated by DPAPI with LocalAlloc and is not used again.
        unsafe { LocalFree(out.data.cast()) };
        v
    }

    pub fn protect(plain: &[u8]) -> Option<Vec<u8>> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut out = DataBlob {
            len: 0,
            data: ptr::null_mut(),
        };
        // SAFETY: every pointer is valid for the call; DPAPI only reads `input` and `entropy`.
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        (ok != 0).then(|| take(out))
    }

    pub fn unprotect(protected: &[u8]) -> Option<Vec<u8>> {
        let input = blob(protected);
        let entropy = blob(ENTROPY);
        let mut out = DataBlob {
            len: 0,
            data: ptr::null_mut(),
        };
        // SAFETY: as in `protect`; no description is requested.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        (ok != 0).then(|| take(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_values_load_as_they_are() {
        assert_eq!(unprotect("ABCD-1234").as_deref(), Some("ABCD-1234"));
        assert!(!is_protected("ABCD-1234"));
    }

    #[cfg(windows)]
    #[test]
    fn roundtrip_and_damaged_blobs() {
        let stored = protect("AB12-CDEF-GHJK-MNPQ-RSTV");
        assert!(is_protected(&stored), "{stored}");
        assert!(!stored.contains("AB12"));
        assert_eq!(
            unprotect(&stored).as_deref(),
            Some("AB12-CDEF-GHJK-MNPQ-RSTV")
        );
        // Damaged or foreign blobs give nothing (never a wrong code).
        let mut bad = stored.clone();
        bad.truncate(bad.len() - 8);
        bad.push_str("AAAAAAA=");
        assert_eq!(unprotect(&bad), None);
        assert_eq!(unprotect("dpapi:não é base64"), None);
    }
}
