use crate::types::{RequestError, ID_LEN_LIMIT, METHOD_LEN_LIMIT};
use bytes::Bytes;
use serde_json::value::RawValue;
use std::ops::Range;

/// Utf8 payload, partially deserialized
#[derive(Clone)]
pub struct Request {
    /// The underlying byte buffer. This is guaranteed to be a validly
    /// formatted JSON string.
    bytes: Bytes,

    /// A range of the `bytes` field that represents the id field of the
    /// JSON-RPC request.
    ///
    /// This is guaranteed to be an accessible, valid, portion of the `bytes`
    /// property, containing validly-formatted JSON.
    ///
    /// This field is generated by deserializing to a [`RawValue`] and then
    /// calculating the offset of the backing slice within the `bytes` field.
    id: Option<Range<usize>>,
    /// A range of the `bytes` field that represents the method field of the
    /// JSON-RPC request.
    ///
    /// This is guaranteed to be an accessible, valid, portion of the `bytes`
    /// property, containing validly-formatted JSON.
    ///
    /// This field is generated by deserializing to a [`RawValue`] and then
    /// calculating the offset of the backing slice within the `bytes` field.
    method: Range<usize>,
    /// A range of the `bytes` field that represents the params field of the
    /// JSON-RPC request.
    ///
    /// This is guaranteed to be an accessible, valid, portion of the `bytes`
    /// property, containing validly-formatted JSON.
    ///
    /// This field is generated by deserializing to a [`RawValue`] and then
    /// calculating the offset of the backing slice within the `bytes` field.
    params: Option<Range<usize>>,
}

impl core::fmt::Debug for Request {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // SAFETY: both str pointers are guaranteed to slices of the owned
        // `bytes` field.

        f.debug_struct("Request")
            .field("bytes", &self.bytes.len())
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

#[derive(serde::Deserialize)]
struct DeserHelper<'a> {
    #[serde(borrow)]
    id: Option<&'a RawValue>,
    #[serde(borrow)]
    method: &'a RawValue,
    #[serde(borrow)]
    params: Option<&'a RawValue>,
}

impl TryFrom<Bytes> for Request {
    type Error = RequestError;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        let DeserHelper { id, method, params } = serde_json::from_slice(bytes.as_ref())?;

        let id = if let Some(id) = id {
            let id = find_range!(bytes, id.get());

            // Ensure the id is not too long
            let id_len = id.end - id.start;
            if id_len > ID_LEN_LIMIT {
                return Err(RequestError::IdTooLarge(id_len));
            }

            Some(id)
        } else {
            None
        };

        // Ensure method is a string, and not too long, and trim the quotes
        // from it
        let method = method
            .get()
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or(RequestError::InvalidMethod)?;
        let method = find_range!(bytes, method);

        let method_len = method.end - method.start;
        if method_len > METHOD_LEN_LIMIT {
            return Err(RequestError::MethodTooLarge(method_len));
        }

        let params = params.map(|params| find_range!(bytes, params.get()));

        Ok(Self {
            bytes,
            id,
            method,
            params,
        })
    }
}

#[cfg(feature = "ws")]
impl TryFrom<tokio_tungstenite::tungstenite::Utf8Bytes> for Request {
    type Error = RequestError;

    fn try_from(bytes: tokio_tungstenite::tungstenite::Utf8Bytes) -> Result<Self, Self::Error> {
        Self::try_from(Bytes::from(bytes))
    }
}

impl Request {
    /// Return a reference to the serialized ID field. If the ID field is
    /// missing, this will return `"null"`, ensuring that response correctly
    /// have a null ID, as per [the JSON-RPC spec].
    ///
    /// [the JSON-RPC spec]: https://www.jsonrpc.org/specification#response_object
    pub fn id(&self) -> Option<&str> {
        self.id.as_ref().map(|range| {
            // SAFETY: `range` is guaranteed to be valid JSON, and a valid
            // slice of `bytes`.
            unsafe { core::str::from_utf8_unchecked(self.bytes.get_unchecked(range.clone())) }
        })
    }

    /// Return an owned version of the serialized ID field.
    pub fn id_owned(&self) -> Option<Box<RawValue>> {
        self.id()
            .map(str::to_string)
            .map(RawValue::from_string)
            .transpose()
            .expect("valid json")
    }

    /// True if the request is a notification, false otherwise.
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// Return a reference to the method str, deserialized.
    ///
    /// This is the method without the preceding and trailing quotes. E.g. if
    /// the method is `foo`, this will return `&"foo"`.
    pub fn method(&self) -> &str {
        // SAFETY: `method` is guaranteed to be valid UTF-8,
        // and a valid slice of `bytes`.
        unsafe { core::str::from_utf8_unchecked(self.bytes.get_unchecked(self.method.clone())) }
    }

    /// Return a reference to the raw method str, with preceding and trailing
    /// quotes. This is effectively the method as a [`RawValue`].
    ///
    /// E.g. if the method is `foo`, this will return `&r#""foo""#`.
    pub fn raw_method(&self) -> &str {
        // SAFETY: `params` is guaranteed to be valid JSON,
        // and a valid slice of `bytes`.
        unsafe {
            core::str::from_utf8_unchecked(
                self.bytes
                    .get_unchecked(self.method.start - 1..self.method.end + 1),
            )
        }
    }

    /// Return a reference to the serialized params field.
    pub fn params(&self) -> &str {
        if let Some(range) = &self.params {
            // SAFETY: `range` is guaranteed to be valid JSON, and a valid
            // slice of `bytes`.
            unsafe { core::str::from_utf8_unchecked(self.bytes.get_unchecked(range.clone())) }
        } else {
            "null"
        }
    }

    /// Deserialize the params field into a type.
    pub fn deser_params<'a: 'de, 'de, T: serde::Deserialize<'de>>(
        &'a self,
    ) -> serde_json::Result<T> {
        serde_json::from_str(self.params())
    }
}

#[cfg(test)]
mod test {
    use crate::types::METHOD_LEN_LIMIT;

    use super::*;

    #[test]
    fn test_request() {
        let bytes = Bytes::from_static(b"{\"id\":1,\"method\":\"foo\",\"params\":[]}");
        let req = Request::try_from(bytes).unwrap();

        assert_eq!(req.id(), Some("1"));
        assert_eq!(req.method(), r#"foo"#);
        assert_eq!(req.params(), r#"[]"#);
    }

    #[test]
    fn non_utf8() {
        let bytes = Bytes::from_static(b"{\"id\xFF\xFF\":1,\"method\":\"foo\",\"params\":[]}");
        let err = Request::try_from(bytes).unwrap_err();

        assert!(matches!(err, RequestError::InvalidJson(_)));
        assert!(err.to_string().contains("invalid unicode code point"));
    }

    #[test]
    fn too_large_id() {
        let id = "a".repeat(ID_LEN_LIMIT + 1);
        let bytes = Bytes::from(format!(r#"{{"id":"{}","method":"foo","params":[]}}"#, id));
        let RequestError::IdTooLarge(size) = Request::try_from(bytes).unwrap_err() else {
            panic!("Expected RequestError::IdTooLarge")
        };

        assert_eq!(size, ID_LEN_LIMIT + 3);
    }

    #[test]
    fn too_large_method() {
        let method = "a".repeat(METHOD_LEN_LIMIT + 1);
        let bytes = Bytes::from(format!(r#"{{"id":1,"method":"{}","params":[]}}"#, method));
        let RequestError::MethodTooLarge(size) = Request::try_from(bytes).unwrap_err() else {
            panic!("Expected RequestError::MethodTooLarge")
        };

        assert_eq!(size, METHOD_LEN_LIMIT + 1);
    }
}
