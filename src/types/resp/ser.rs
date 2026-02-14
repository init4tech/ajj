use crate::{ResponsePayload, RpcSend};
use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::value::RawValue;

/// Response struct.
#[derive(Debug, Clone)]
pub(crate) struct Response<'a, 'b, T, E> {
    pub(crate) id: &'b RawValue,
    pub(crate) payload: &'a ResponsePayload<T, E>,
}

impl Response<'_, '_, (), ()> {
    /// Parse error response, used when the request is not valid JSON.
    pub(crate) fn parse_error() -> Box<RawValue> {
        Response::<(), ()> {
            id: RawValue::NULL,
            payload: &ResponsePayload::parse_error(),
        }
        .to_json()
    }

    /// Method not found response, used in default fallback handler.
    pub(crate) fn method_not_found(id: &RawValue) -> Box<RawValue> {
        Response::<(), ()> {
            id,
            payload: &ResponsePayload::method_not_found(),
        }
        .to_json()
    }

    /// Method not found response, used in default fallback handler. This
    /// function exists to simplify notification responses, which should be
    /// omitted.
    pub(crate) fn maybe_method_not_found(id: Option<&RawValue>) -> Option<Box<RawValue>> {
        id.map(Self::method_not_found)
    }

    /// Response failed to serialize
    pub(crate) fn serialization_failure(id: &RawValue) -> Box<RawValue> {
        RawValue::from_string(format!(
            r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32700,"message":"response serialization error"}}}}"#,
            id.get()
        ))
        .expect("valid json")
    }

    /// Build a JSON-RPC response from an id and a payload.
    ///
    /// The payload is consumed and pre-serialized via
    /// [`RpcSend::into_raw_value`], then embedded directly into the
    /// JSON-RPC envelope string without a second serialization pass.
    pub(crate) fn build_response<T, E>(
        id: Option<&RawValue>,
        payload: ResponsePayload<T, E>,
    ) -> Option<Box<RawValue>>
    where
        T: RpcSend,
        E: RpcSend,
    {
        let id = id?;
        let raw = match payload.into_raw() {
            Ok(raw) => raw,
            Err(err) => {
                tracing::debug!(%err, ?id, "failed to serialize response payload");
                return Some(Self::serialization_failure(id));
            }
        };
        let json = match raw.0 {
            Ok(result) => {
                format!(
                    r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#,
                    id.get(),
                    result.get()
                )
            }
            Err(error) => match serde_json::to_string(&error) {
                Ok(error_json) => {
                    format!(
                        r#"{{"jsonrpc":"2.0","id":{},"error":{}}}"#,
                        id.get(),
                        error_json,
                    )
                }
                Err(err) => {
                    tracing::debug!(%err, ?id, "failed to serialize error payload");
                    return Some(Self::serialization_failure(id));
                }
            },
        };
        Some(RawValue::from_string(json).unwrap_or_else(|err| {
            tracing::debug!(%err, ?id, "failed to construct response");
            Self::serialization_failure(id)
        }))
    }
}

impl<'a, 'b, T, E> Response<'a, 'b, T, E>
where
    T: Serialize,
    E: Serialize,
{
    fn to_json(&self) -> Box<RawValue> {
        serde_json::value::to_raw_value(self).unwrap_or_else(|err| {
            tracing::debug!(%err, id = ?self.id, "failed to serialize response");
            Response::serialization_failure(self.id)
        })
    }
}

impl<T, E> Serialize for Response<'_, '_, T, E>
where
    T: Serialize,
    E: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("jsonrpc", "2.0")?;
        map.serialize_entry("id", &self.id)?;
        match &self.payload {
            ResponsePayload(Ok(result)) => {
                map.serialize_entry("result", result)?;
            }
            ResponsePayload(Err(error)) => {
                map.serialize_entry("error", error)?;
            }
        }
        map.end()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::assert_rv_eq;

    #[test]
    fn ser_failure() {
        let id = RawValue::from_string("1".to_string()).unwrap();
        let res = Response::<(), ()>::serialization_failure(&id);
        assert_rv_eq(
            &res,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32700,"message":"response serialization error"}}"#,
        );
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `alloy` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/alloy-rs/alloy
//
// The MIT License (MIT)
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.
