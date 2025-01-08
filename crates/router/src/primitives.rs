use std::ops::{Add, AddAssign};

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
