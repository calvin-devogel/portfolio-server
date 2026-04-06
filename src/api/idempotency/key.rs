// let's remind ourselves of what is happening here
// this is the idempotency key, associated with any action we're trying
// execute idempotently
#[derive(Debug)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    const MAX_LENGTH: usize = 50;
}

// we need a TryFrom to ensure the key fits our criteria, specifically:
// - the key must be non-empty
// - the key must be no more than 50 characters in length
impl TryFrom<String> for IdempotencyKey {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            anyhow::bail!("The idempotency key cannot be empty.")
        }
        if s.len() >= Self::MAX_LENGTH {
            anyhow::bail!(format!(
                "The idempotency key must be shorter than {} characters",
                Self::MAX_LENGTH
            ));
        }
        Ok(Self(s))
    }
}

// key -> String (consumes)
// ex: `let s: String = idempotency_key.into()`
// or: `String::from(idempotency_key)`
impl From<IdempotencyKey> for String {
    fn from(k: IdempotencyKey) -> Self {
        k.0
    }
}

// provides a *borrowed* reference to the string inside the key
// without consuming it.
// allows us to pass a key to any function that accepts an AsRef<str>
// for example `fn some_function(s: impl AsRef<str>)`
impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_try_from() {
        let key = IdempotencyKey::try_from("valid_key".to_string()).unwrap();
        assert_eq!(key.as_ref(), "valid_key");

        let empty_key = IdempotencyKey::try_from("".to_string());
        assert!(empty_key.is_err());

        let long_key = "a".repeat(IdempotencyKey::MAX_LENGTH + 1);
        let long_key_result = IdempotencyKey::try_from(long_key);
        assert!(long_key_result.is_err());
    }

    #[test]
    fn string_from_key() {
        let key = IdempotencyKey::try_from("another_valid_key".to_string()).unwrap();
        let s: String = key.into();
        assert_eq!(s, "another_valid_key");
    }
}
