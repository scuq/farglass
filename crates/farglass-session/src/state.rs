//! The state store of the session: JSON values by key, for the shell.
//!
//! The shell keeps its state here, for example the open windows. The store
//! lives as long as the agent, thus the shell gets the state back after a
//! reload, after a break of the connection, and from a different browser.

use std::collections::HashMap;

use serde_json::Value;

/// The largest size of all values together, in bytes of JSON.
pub const MAX_TOTAL_LEN: usize = 8 << 20;

/// The longest key in bytes.
pub const MAX_KEY_LEN: usize = 256;

/// An error of a set request.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    /// The key is empty or longer than [`MAX_KEY_LEN`].
    #[error("the key must have 1 to {MAX_KEY_LEN} bytes")]
    Key,
    /// The new value makes the store larger than [`MAX_TOTAL_LEN`].
    #[error("the state store is full")]
    Full,
}

/// The values of the session.
#[derive(Debug, Default)]
pub struct StateStore {
    values: HashMap<String, (Value, usize)>,
    total: usize,
}

impl StateStore {
    /// Returns the value of `key`, or `null`.
    #[must_use]
    pub fn get(&self, key: &str) -> Value {
        self.values
            .get(key)
            .map_or(Value::Null, |(value, _)| value.clone())
    }

    /// Sets the value of `key`. `null` removes the key.
    ///
    /// # Errors
    ///
    /// [`StateError`] if the key is not valid or the store is full. The store does not change.
    pub fn set(&mut self, key: &str, value: Value) -> Result<(), StateError> {
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(StateError::Key);
        }
        let old = self.values.get(key).map_or(0, |(_, len)| *len);
        if value.is_null() {
            self.values.remove(key);
            self.total -= old;
            return Ok(());
        }
        let len = key.len() + value.to_string().len();
        if self.total - old + len > MAX_TOTAL_LEN {
            return Err(StateError::Full);
        }
        self.total = self.total - old + len;
        self.values.insert(key.to_owned(), (value, len));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn set_get_and_remove() {
        let mut store = StateStore::default();
        assert_eq!(store.get("windows"), Value::Null);
        store.set("windows", json!([{"app": "terminal"}])).unwrap();
        assert_eq!(store.get("windows"), json!([{"app": "terminal"}]));
        store.set("windows", Value::Null).unwrap();
        assert_eq!(store.get("windows"), Value::Null);
        assert_eq!(store.total, 0);
    }

    #[test]
    fn size_and_key_limits() {
        let mut store = StateStore::default();
        assert_eq!(store.set("", json!(1)), Err(StateError::Key));
        assert_eq!(
            store.set(&"k".repeat(MAX_KEY_LEN + 1), json!(1)),
            Err(StateError::Key)
        );
        let big = "x".repeat(MAX_TOTAL_LEN / 2);
        store.set("a", json!(big)).unwrap();
        assert_eq!(store.set("b", json!(big)), Err(StateError::Full));
        // A replacement of the same key counts the old size only one time.
        store.set("a", json!(big)).unwrap();
        assert_eq!(store.get("b"), Value::Null);
    }
}
