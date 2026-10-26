//! Cifra de credenciais com a conta do utilizador do Windows (DPAPI) — formato compatível com a versão Python
//! (hexadecimal do blob devolvido por CryptProtectData).

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as *mut _);
        v
    }

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let input = blob(data);
            let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
            if CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) == 0 {
                return Err("Falha ao encriptar credenciais com Windows DPAPI.".into());
            }
            Ok(take(out))
        }
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let input = blob(data);
            let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
            if CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) == 0 {
                return Err("Falha ao decifrar credenciais com Windows DPAPI.".into());
            }
            Ok(take(out))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(_: &[u8]) -> Result<Vec<u8>, String> {
        Err("DPAPI só existe no Windows.".into())
    }
    pub fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> {
        Err("DPAPI só existe no Windows.".into())
    }
}

/// Encripta texto com a chave da conta Windows atual; devolve o blob em hexadecimal.
pub fn encrypt(text: &str) -> Result<String, String> {
    imp::protect(text.as_bytes()).map(hex::encode)
}

/// Decifra o hexadecimal produzido por `encrypt` (ou pela versão Python).
pub fn decrypt(hex_data: &str) -> Result<String, String> {
    let bytes = hex::decode(hex_data.trim()).map_err(|_| "Credenciais guardadas inválidas.".to_string())?;
    let plain = imp::unprotect(&bytes)?;
    String::from_utf8(plain).map_err(|_| "Credenciais guardadas inválidas.".to_string())
}
