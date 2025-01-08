use std::borrow::Cow;

/// Errors that can occur when registering a method.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RegistrationError {
    #[error("Method already registered: {0}")]
    MethodAlreadyRegistered(Cow<'static, str>),
}

impl RegistrationError {
    /// Create a new `MethodAlreadyRegistered` error.
    pub fn method_already_registered(name: impl Into<Cow<'static, str>>) -> Self {
        Self::MethodAlreadyRegistered(name.into())
    }
}
