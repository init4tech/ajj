use core::ops::{Add, AddAssign};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// A unique internal identifier for a method.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct MethodId(usize);

impl From<usize> for MethodId {
    fn from(id: usize) -> Self {
        Self(id)
    }
}

impl Add<usize> for MethodId {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign<usize> for MethodId {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

/// An object that can be sent over RPC.
///
/// This marker trait is blanket-implemented for every qualifying type. It is
/// used to indicate that a type can be sent in the body of a JSON-RPC message.
///
/// Note that this trait does **not** require [`Clone`] or [`Debug`]. Types
/// that serialize computed or lazily-produced sequences can implement
/// [`Serialize`] directly and be returned from handlers without collecting
/// into a [`Vec`] first.
///
/// # Example: serializing an iterator without allocation
///
/// ```
/// use ajj::RpcSend;
/// use serde::{Serialize, ser::SerializeSeq, Serializer};
/// use std::sync::Mutex;
///
/// /// Wraps an iterator, serializing its items as a JSON array
/// /// without collecting into a [`Vec`].
/// struct IterResponse<T>(Mutex<Option<T>>);
///
/// impl<'a, T> Serialize for IterResponse<T>
/// where
///     T: Iterator<Item = &'a usize>,
/// {
///     fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
///         let mut seq = serializer.serialize_seq(None)?;
///         if let Some(iter) = self.0.lock().unwrap().take() {
///             for item in iter {
///                 seq.serialize_element(item)?;
///             }
///         }
///         seq.end()
///     }
/// }
///
/// let data = vec![1usize, 2, 3];
/// let resp = IterResponse(Mutex::new(Some(data.iter())));
///
/// // IterResponse satisfies RpcSend without implementing Clone or Debug.
/// fn is_rpc_send(_: &impl RpcSend) {}
/// is_rpc_send(&resp);
///
/// let json = serde_json::to_string(&resp).unwrap();
/// assert_eq!(json, "[1,2,3]");
/// ```
pub trait RpcSend: Serialize + Send + Sync + Unpin {}

impl<T> RpcSend for T where T: Serialize + Send + Sync + Unpin {}

/// An object that can be received over RPC.
///
/// This marker trait is blanket-implemented for every qualifying type. It is
/// used to indicate that a type can be received in the body of a JSON-RPC
/// message.
///
/// # Note
///
/// We add the `'static` lifetime to the supertraits to indicate that the type
/// can't borrow. This is a simplification that makes it easier to use the
/// types in client code. Servers may prefer borrowing, using the [`RpcBorrow`]
/// trait.
pub trait RpcRecv: DeserializeOwned + Send + Sync + Unpin + 'static {}

impl<T> RpcRecv for T where T: DeserializeOwned + Send + Sync + Unpin + 'static {}

/// An object that can be received over RPC, borrowing from the the
/// deserialization context.
///
/// This marker trait is blanket-implemented for every qualifying type. It is
/// used to indicate that a type can be borrowed from the body of a wholly or
/// partially serialized JSON-RPC message.
pub trait RpcBorrow<'de>: Deserialize<'de> + Send + Sync + Unpin {}

impl<'de, T> RpcBorrow<'de> for T where T: Deserialize<'de> + Send + Sync + Unpin {}

/// An object that can be both sent and received over RPC.
///
/// This marker trait is blanket-implemented for every qualifying type. It is
/// used to indicate that a type can be both sent and received in the body of a
/// JSON-RPC message.
///
/// # Note
///
/// We add the `'static` lifetime to the supertraits to indicate that the type
/// can't borrow. This is a simplification that makes it easier to use the
/// types in client code. Servers may prefer borrowing, using the
/// [`BorrowedRpcObject`] trait.
pub trait RpcObject: RpcSend + RpcRecv {}

impl<T> RpcObject for T where T: RpcSend + RpcRecv {}

/// An object that can be both sent and received over RPC, borrowing from the
/// the deserialization context.
///
/// This marker trait is blanket-implemented for every qualifying type. It is
/// used to indicate that a type can be both sent and received in the body of a
/// JSON-RPC message, and can borrow from the deserialization context.
pub trait BorrowedRpcObject<'de>: RpcBorrow<'de> + RpcSend {}

impl<'de, T> BorrowedRpcObject<'de> for T where T: RpcBorrow<'de> + RpcSend {}
