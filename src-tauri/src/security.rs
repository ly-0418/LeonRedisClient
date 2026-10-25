use std::ptr;

use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("保存的密码数据格式无效".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| "保存的密码数据格式无效".to_string())?;
        bytes.push(byte);
    }
    Ok(bytes)
}

pub fn protect(plain: &str) -> Result<String, String> {
    let input = blob(plain.as_bytes());
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };

    if ok == 0 {
        return Err(format!(
            "使用 Windows 凭据加密密码失败: {}",
            std::io::Error::last_os_error()
        ));
    }

    let cipher =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as HLOCAL);
    }
    Ok(encode_hex(&cipher))
}

pub fn unprotect(encoded: &str) -> Result<String, String> {
    let cipher = decode_hex(encoded)?;
    let input = blob(&cipher);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let ok = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };

    if ok == 0 {
        return Err(format!(
            "使用 Windows 凭据解密密码失败: {}",
            std::io::Error::last_os_error()
        ));
    }

    let plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as HLOCAL);
    }
    String::from_utf8(plain).map_err(|_| "保存的密码不是有效文本".to_string())
}

#[cfg(test)]
mod tests {
    use super::{protect, unprotect};

    #[test]
    fn dpapi_round_trip() {
        let encrypted = protect("leon-redis-test-password").expect("protect");
        assert!(!encrypted.contains("leon-redis-test-password"));
        let decrypted = unprotect(&encrypted).expect("unprotect");
        assert_eq!(decrypted, "leon-redis-test-password");
    }
}
