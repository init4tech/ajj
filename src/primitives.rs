use core::ops::{Add, AddAssign};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;

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
/// This trait is blanket-implemented for every [`Serialize`] type that
/// satisfies the required bounds. It is used to indicate that a type can
/// be sent in the body of a JSON-RPC message.
///
/// The [`into_raw_value`] method consumes `self` and produces a serialized
/// [`RawValue`]. This consuming interface allows types like iterators to
/// be serialized without intermediate allocation (e.g., collecting into a
/// [`Vec`]).
///
/// Note that this trait does **not** require [`Clone`] or [`Debug`].
///
/// # Custom implementations
///
/// Types that do not implement [`Serialize`] can implement `RpcSend`
/// directly by providing a custom [`into_raw_value`]. This is useful for
/// lazily-produced sequences or pre-serialized data.
///
/// # Example: serializing an iterator without allocation
///
/// ```
/// use ajj::RpcSend;
/// use serde::Serialize;
/// use serde_json::value::RawValue;
///
/// /// Wraps an iterator, serializing its items as a JSON array
/// /// without collecting into a [`Vec`].
/// struct IterResponse<T>(T);
///
/// impl<T, Item> RpcSend for IterResponse<T>
/// where
///     T: Iterator<Item = Item> + Send + Sync + Unpin,
///     Item: Serialize,
/// {
///     fn into_raw_value(self) -> serde_json::Result<Box<RawValue>> {
///         let mut json = String::from("[");
///         for (i, item) in self.0.enumerate() {
///             if i > 0 {
///                 json.push(',');
///             }
///             json.push_str(&serde_json::to_string(&item)?);
///         }
///         json.push(']');
///         RawValue::from_string(json)
///     }
/// }
///
/// let data = vec![1usize, 2, 3];
/// let resp = IterResponse(data.into_iter());
///
/// let json = resp.into_raw_value().unwrap();
/// assert_eq!(json.get(), "[1,2,3]");
/// ```
///
/// [`into_raw_value`]: RpcSend::into_raw_value
pub trait RpcSend: Send + Sync + Unpin {
    /// Consume this value and serialize it into a [`RawValue`].
    fn into_raw_value(self) -> serde_json::Result<Box<RawValue>>;
}

impl<T> RpcSend for T
where
    T: Serialize + Send + Sync + Unpin,
{
    fn into_raw_value(self) -> serde_json::Result<Box<RawValue>> {
        serde_json::value::to_raw_value(&self)
    }
}

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
