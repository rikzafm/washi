use crate::platform::{KeyProtector, ProtectError};
use windows_sys::Win32::Foundation::LocalFree;
use zeroize::Zeroizing;
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, CryptProtectData, CryptUnprotectData, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// Fixed application entropy: binds blobs to Washi, not just to the Windows account.
const ENTROPY: &[u8] = b"Washi.key.v1";

pub struct DpapiProtector;

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

/// Copy DPAPI's output into memory that is wiped when dropped, wipe DPAPI's own buffer (it may hold
/// the plaintext key) and free it.
fn take(out: CRYPT_INTEGER_BLOB) -> Zeroizing<Vec<u8>> {
    // SAFETY: DPAPI returned a LocalAlloc'd buffer of `cbData` bytes, which we copy, wipe and free.
    unsafe {
        let v = Zeroizing::new(std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec());
        std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
        LocalFree(out.pbData as _);
        v
    }
}

impl KeyProtector for DpapiProtector {
    fn generate_key(&self) -> Result<Zeroizing<[u8; 32]>, ProtectError> {
        let mut key = Zeroizing::new([0u8; 32]);
        // SAFETY: buffer is valid for 32 bytes; null algorithm + system RNG flag is the documented form.
        let status = unsafe {
            BCryptGenRandom(std::ptr::null_mut(), key.as_mut_ptr(), 32, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
        };
        if status < 0 {
            return Err(ProtectError(format!("BCryptGenRandom failed ({status:#x})")));
        }
        Ok(key)
    }

    fn protect(&self, raw: &[u8]) -> Result<Vec<u8>, ProtectError> {
        let input = blob(raw);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: all pointers refer to live blobs for the duration of the call.
        let ok = unsafe {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(ProtectError(format!("CryptProtectData failed ({})", std::io::Error::last_os_error())));
        }
        Ok(std::mem::take(&mut *take(out)))
    }

    fn unprotect(&self, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, ProtectError> {
        let input = blob(wrapped);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: as above.
        let ok = unsafe {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(ProtectError(format!("CryptUnprotectData failed ({})", std::io::Error::last_os_error())));
        }
        Ok(take(out))
    }
}
