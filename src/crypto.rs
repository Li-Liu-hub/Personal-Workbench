//! Compatibility decoder for old encrypted account files.
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use zeroize::Zeroizing;

use crate::error::{DeskError, Result};

pub struct VaultKey(pub Zeroizing<[u8; 32]>);

#[cfg(test)]
pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|_| DeskError::Crypto)?;
    Ok(bytes)
}

/// Protect a local vault key with the current Windows user's DPAPI context.
/// This is automatic and does not introduce an application login prompt.
#[cfg(all(windows, test))]
pub fn protect_local_key(key: &[u8; 32]) -> Result<Vec<u8>> {
    dpapi(key, true)
}

/// Open a local vault key using the current Windows user's DPAPI context.
#[cfg(windows)]
pub fn unprotect_local_key(protected: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let plaintext = Zeroizing::new(dpapi(protected, false)?);
    let key = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| DeskError::Crypto)?;
    Ok(Zeroizing::new(key))
}

#[cfg(windows)]
fn dpapi(data: &[u8], protect: bool) -> Result<Vec<u8>> {
    use std::{ptr, slice};

    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    use zeroize::Zeroize;

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(|_| DeskError::Crypto)?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let succeeded = unsafe {
        if protect {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if succeeded == 0 || output.pbData.is_null() {
        if !output.pbData.is_null() {
            if !protect && output.cbData > 0 {
                unsafe {
                    slice::from_raw_parts_mut(output.pbData, output.cbData as usize).zeroize();
                }
            }
            unsafe {
                LocalFree(output.pbData.cast());
            }
        }
        return Err(DeskError::Crypto);
    }
    let bytes = unsafe { slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        if !protect && output.cbData > 0 {
            slice::from_raw_parts_mut(output.pbData, output.cbData as usize).zeroize();
        }
        LocalFree(output.pbData.cast());
    }
    Ok(bytes)
}

#[cfg(all(not(windows), test))]
pub fn protect_local_key(_key: &[u8; 32]) -> Result<Vec<u8>> {
    Err(DeskError::Crypto)
}

#[cfg(not(windows))]
pub fn unprotect_local_key(_protected: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    Err(DeskError::Crypto)
}

#[cfg(test)]
pub fn encrypt(
    key: &VaultKey,
    key_id: &str,
    record_id: &str,
    plaintext: &[u8],
    nonce: &[u8; 12],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(&key.0[..]).map_err(|_| DeskError::Crypto)?;
    let aad = associated_data(key_id, record_id);
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| DeskError::Crypto)
}

pub fn decrypt(
    key: &VaultKey,
    key_id: &str,
    record_id: &str,
    ciphertext: &[u8],
    nonce: &[u8; 12],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(&key.0[..]).map_err(|_| DeskError::Crypto)?;
    let aad = associated_data(key_id, record_id);
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| DeskError::Crypto)
}

fn associated_data(key_id: &str, record_id: &str) -> Vec<u8> {
    format!("PersonalDesk/entry/v1/{key_id}/{record_id}").into_bytes()
}
