use crate::{types::resp::IntoErrorPayload, ResponsePayload, RpcSend};

/// Internal trait unifying `Result<T, E>` and `ResponsePayload<T, E>`
/// conversion in the handler macro.
pub(crate) trait IntoResponsePayload {
    /// The successful payload type.
    type Payload: RpcSend;
    /// The error data type.
    type ErrData: RpcSend;

    /// Convert this value into a [`ResponsePayload`].
    fn into_response_payload(self) -> ResponsePayload<Self::Payload, Self::ErrData>;
}

impl<T: RpcSend, E: RpcSend> IntoResponsePayload for ResponsePayload<T, E> {
    type Payload = T;
    type ErrData = E;

    fn into_response_payload(self) -> ResponsePayload<T, E> {
        self
    }
}

impl<T: RpcSend, E: IntoErrorPayload> IntoResponsePayload for Result<T, E> {
    type Payload = T;
    type ErrData = E::ErrData;

    fn into_response_payload(self) -> ResponsePayload<T, E::ErrData> {
        match self {
            Ok(ok) => ResponsePayload(Ok(ok)),
            Err(err) => ResponsePayload(Err(err.into_error_payload())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_payload_identity() {
        let rp = ResponsePayload::<u32, ()>(Ok(42));
        let result = rp.into_response_payload();
        assert_eq!(result.as_success(), Some(&42));
    }

    #[test]
    fn result_ok_converts() {
        let r: Result<u32, &str> = Ok(42);
        let rp = r.into_response_payload();
        assert_eq!(rp.as_success(), Some(&42));
    }

    #[test]
    fn result_err_uses_into_error_payload() {
        let r: Result<u32, &str> = Err("bad");
        let rp = r.into_response_payload();
        let err = rp.as_error().unwrap();
        assert_eq!(err.code, -32603);
        assert_eq!(err.message, "bad");
    }
}
