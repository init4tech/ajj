use crate::types::{Request, RequestError};
use bytes::Bytes;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::ops::Range;
use tracing::{debug, enabled, instrument, span::Span, Level};

/// UTF-8, partially deserialized JSON-RPC request batch.
#[derive(Default)]
pub struct InboundData {
    /// The underlying byte buffer. This is guaranteed to be a validly formatted
    /// JSON string, containing either a single request or a batch of requests.
    bytes: Bytes,

    /// Ranges of the `bytes` field that represent the JSON objects in the
    /// inbound data.
    reqs: Vec<Range<usize>>,

    /// Whether the batch was a single request. This will be true if the batch
    /// contained a single JSON object i.e. `{ .. }` and false if it was an
    /// array.
    single: bool,
}

impl InboundData {
    /// Returns the number of JSON objects in the batch.
    pub(crate) fn len(&self) -> usize {
        self.reqs.len()
    }

    /// Returns an iterator over the requests in the batch.
    pub(crate) fn iter(&self) -> impl Iterator<Item = Result<Request, RequestError>> + '_ {
        self.reqs
            .iter()
            .map(move |r| Request::try_from(self.bytes.slice(r.clone())))
    }

    /// Returns true if the batch was a single request, i.e. not an array.
    pub const fn single(&self) -> bool {
        self.single
    }
}

impl core::fmt::Debug for InboundData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BatchReq")
            .field("bytes", &self.bytes.len())
            .field("reqs", &self.reqs.len())
            .finish()
    }
}

impl TryFrom<Bytes> for InboundData {
    type Error = RequestError;

    #[instrument(level = "debug", skip(bytes), fields(buf_len = bytes.len(), bytes = tracing::field::Empty))]
    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        if enabled!(Level::TRACE) {
            Span::current().record("bytes", format!("0x{:x}", bytes));
        }

        // This event exists only so that people who use default console
        // logging setups still see the span details. Without this event, the
        // span would not show up in logs.
        debug!("Parsing inbound data");

        // We set up the deserializer to read from the byte buffer.
        let mut deserializer = serde_json::Deserializer::from_slice(&bytes);

        // If we succesfully deser a batch, we can return it.
        if let Ok(reqs) = Vec::<&RawValue>::deserialize(&mut deserializer) {
            // `.end()` performs trailing charcter checks
            deserializer.end()?;
            let reqs = reqs
                .into_iter()
                .map(|raw| find_range!(bytes, raw.get()))
                .collect();

            return Ok(Self {
                bytes,
                reqs,
                single: false,
            });
        }

        // If it's not a batch, it should be a single request.
        let rv = <&RawValue>::deserialize(&mut deserializer)?;

        // `.end()` performs trailing charcter checks
        deserializer.end()?;

        // If not a JSON object, return an error.
        if !rv.get().starts_with("{") {
            return Err(RequestError::UnexpectedJsonType);
        }

        let range = find_range!(bytes, rv.get());

        Ok(Self {
            bytes,
            reqs: vec![range],
            single: true,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn assert_invalid_json(batch: &'static str) {
        let bytes = Bytes::from(batch);
        let err = InboundData::try_from(bytes).unwrap_err();

        assert!(matches!(err, RequestError::InvalidJson(_)));
    }

    #[test]
    fn test_deser_batch() {
        let batch = r#"[
            {"id": 1, "method": "foo", "params": [1, 2, 3]},
            {"id": 2, "method": "bar", "params": [4, 5, 6]}
        ]"#;

        let bytes = Bytes::from(batch);
        let batch = InboundData::try_from(bytes).unwrap();

        assert_eq!(batch.len(), 2);
        assert!(!batch.single());
    }

    #[test]
    fn test_deser_single() {
        let single = r#"{"id": 1, "method": "foo", "params": [1, 2, 3]}"#;

        let bytes = Bytes::from(single);
        let batch = InboundData::try_from(bytes).unwrap();

        assert_eq!(batch.len(), 1);
        assert!(batch.single());
    }

    #[test]
    fn test_deser_single_with_whitespace() {
        let single = r#"

        {"id": 1, "method": "foo", "params": [1, 2, 3]}

                "#;

        let bytes = Bytes::from(single);
        let batch = InboundData::try_from(bytes).unwrap();

        assert_eq!(batch.len(), 1);
        assert!(batch.single());
    }

    #[test]
    fn test_broken_batch() {
        let batch = r#"[
            {"id": 1, "method": "foo", "params": [1, 2, 3]},
            {"id": 2, "method": "bar", "params": [4, 5, 6]
        ]"#;

        assert_invalid_json(batch);
    }

    #[test]
    fn test_junk_prefix() {
        let batch = r#"JUNK[
            {"id": 1, "method": "foo", "params": [1, 2, 3]},
            {"id": 2, "method": "bar", "params": [4, 5, 6]}
        ]"#;

        assert_invalid_json(batch);
    }

    #[test]
    fn test_junk_suffix() {
        let batch = r#"[
            {"id": 1, "method": "foo", "params": [1, 2, 3]},
            {"id": 2, "method": "bar", "params": [4, 5, 6]}
        ]JUNK"#;

        assert_invalid_json(batch);
    }

    #[test]
    fn test_invalid_utf8_prefix() {
        let batch = r#"\xF1\x80[
            {"id": 1, "method": "foo", "params": [1, 2, 3]},
            {"id": 2, "method": "bar", "params": [4, 5, 6]}
        ]"#;

        assert_invalid_json(batch);
    }

    #[test]
    fn test_invalid_utf8_suffix() {
        let batch = r#"[
            {"id": 1, "method": "foo", "params": [1, 2, 3]},
            {"id": 2, "method": "bar", "params": [4, 5, 6]}
        ]\xF1\x80"#;

        assert_invalid_json(batch);
    }
}
