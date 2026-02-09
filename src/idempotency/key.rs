// let's remind ourselves of what is happening here
// this is the idempotency key, associated with any action we're trying
// execute idempotently
#[derive(Debug)]
pub struct IdempotencyKey(String);

// we need a TryFrom to ensure the key fits our criteria, specifically:
// - the key must be non-empty
// - the key must be no more than 50 characters in length
impl TryFrom<String> for IdempotencyKey {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            anyhow::bail!("The idempotency key cannot be empty.")
        }
        let max_length = 50;
        if s.len() >= max_length {
            anyhow::bail!("The idempotency key must be shorter than {max_length} characters");
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
