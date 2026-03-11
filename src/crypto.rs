use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        // another acceptable .expect(), there's no reason for us
        // to test AES-GCM itself, so assume this always works
        .expect("AES-GCM encrypt failed");
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    anyhow::ensure!(data.len() > 12, "Ciphertext too short");
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // fake key to test decryption
    const KEY: &[u8; 32] = b"KKVdjF4YnQKhuikgbUzR4HRjOZPzDzfq";

    #[test]
    fn data_too_short() {
        let result = decrypt(KEY, &[0u8; 12]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Ciphertext too short");
    }

    #[test]
    fn ciphertext_is_corrupted() {
        let mut ciphertext = encrypt(KEY, b"hello").unwrap();
        *ciphertext.last_mut().unwrap() ^= 0xFF;
        let result = decrypt(KEY, &ciphertext);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Decryption failed");
    }
}
