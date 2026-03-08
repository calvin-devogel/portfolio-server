use actix_web::{HttpResponse, http::header::LOCATION};
use aws_lc_rs::{
    aead::{
        self,
        AES_256_GCM,
        BoundKey,
        NonceSequence,
        SealingKey,
        OpeningKey,
        UnboundKey,
        Nonce,
        NONCE_LEN
    }
};

// http 400 aka client-side error
pub fn e400<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorBadRequest(e)
}

// http 500 aka server-side error
pub fn e500<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}

// redirect (don't think I need this on the server side, probably have to send a signal?)
#[must_use]
pub fn see_other(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, location))
        .finish()
}

#[must_use]
pub fn unauthorized() -> HttpResponse {
    HttpResponse::Unauthorized().finish()
}

// format the error chain
#[allow(clippy::missing_errors_doc)]
pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{e}\n")?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{cause}")?;
        current = cause.source();
    }
    Ok(())
}

struct OneNonce(Option<Nonce>);

impl NonceSequence for OneNonce {
    fn advance(&mut self) -> Result<Nonce, aws_lc_rs::error::Unspecified> {
        self.0.take().ok_or(aws_lc_rs::error::Unspecified)
    }
}

pub fn encrypt_totp_secret(plaintext: &str, raw_key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    aws_lc_rs::rand::fill(&mut nonce_bytes)?;

    let unbound = UnboundKey::new(&AES_256_GCM, raw_key)?;
    let mut key = SealingKey::new(unbound, OneNonce(Some(Nonce::assume_unique_for_key(nonce_bytes))));

    let mut in_out = plaintext.as_bytes().to_vec();
    key.seal_in_place_append_tag(aead::Aad::empty(), &mut in_out)?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);
    Ok(result)
}

pub fn decrypt_totp_secret(blob: &[u8], raw_key: &[u8; 32]) -> anyhow::Result<String> {
    anyhow::ensure!(blob.len() > NONCE_LEN, "Ciphertext too short");
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);

    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)?;
    let unbound = UnboundKey::new(&AES_256_GCM, raw_key)?;
    let mut key = OpeningKey::new(unbound, OneNonce(Some(nonce)));

    let mut in_out = ciphertext.to_vec();
    let plaintext = key.open_in_place(aead::Aad::empty(), &mut in_out)?;
    Ok(String::from_utf8(plaintext.to_vec())?)
}