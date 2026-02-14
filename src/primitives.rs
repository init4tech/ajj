use core::{
    fmt,
    ops::{Add, AddAssign},
};

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
pub trait RpcSend: Serialize + fmt::Debug + Send + Sync + Unpin {}

impl<T> RpcSend for T where T: Serialize + fmt::Debug + Send + Sync + Unpin {}

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
pub trait RpcRecv: DeserializeOwned + fmt::Debug + Send + Sync + Unpin + 'static {}

impl<T> RpcRecv for T where T: DeserializeOwned + fmt::Debug + Send + Sync + Unpin + 'static {}

/// An object that can be received over RPC, borrowing from the the
/// deserialization context.
///
/// This marker trait is blanket-implemented for every qualifying type. It is
/// used to indicate that a type can be borrowed from the body of a wholly or
/// partially serialized JSON-RPC message.
pub trait RpcBorrow<'de>: Deserialize<'de> + fmt::Debug + Send + Sync + Unpin {}

impl<'de, T> RpcBorrow<'de> for T where T: Deserialize<'de> + fmt::Debug + Send + Sync + Unpin {}

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
