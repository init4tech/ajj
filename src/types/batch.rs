use crate::types::{Request, RequestError};
use bytes::Bytes;
use core::str;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::ops::Range;
use tracing::{debug, enabled, instrument, Level};

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
            tracing::span::Span::current().record("bytes", format!("0x{:x}", bytes));
        }
        debug!("Parsing inbound data");

        // Trim whitespace from the input bytes. This is necessary to ensure that
        // we can parse the input as a JSON string. The JSON spec allows for
        // whitespace before and after the JSON string.
        // Sadly [`Bytes::trim_ascii`] does not remove linebreaks, so we have to
        // convert to a str and trim it.
        let bytes = Bytes::from(str::from_utf8(bytes.as_ref())?.trim().to_owned());

        // Special-case a single request, rejecting invalid JSON.
        if bytes.starts_with(b"{") {
            let rv: &RawValue = serde_json::from_slice(bytes.as_ref())?;

            let range = find_range!(bytes, rv.get());

            return Ok(Self {
                bytes,
                reqs: vec![range],
                single: true,
            });
        }

        // Otherwise, parse the batch
        let DeserHelper(reqs) = serde_json::from_slice(bytes.as_ref())?;
        let reqs = reqs
            .into_iter()
            .map(|raw| find_range!(bytes, raw.get()))
            .collect();

        Ok(Self {
            bytes,
            reqs,
            single: false,
        })
    }
}

#[derive(Debug, Deserialize)]
struct DeserHelper<'a>(#[serde(borrow)] Vec<&'a RawValue>);

#[cfg(test)]
mod test {
    use super::*;

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
}
