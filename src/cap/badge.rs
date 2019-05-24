/// Wrapper for an Endpoint or Notification badge.
/// Note that the kernel will ignore any use of the high 4 bits
#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub struct Badge {
    pub(crate) inner: usize,
}

impl Badge {
    pub fn are_all_overlapping_bits_set(self, other: Badge) -> bool {
        if self.inner == 0 && other.inner == 0 {
            return true;
        }
        let overlap = self.inner & other.inner;
        overlap != 0
    }
}

impl From<usize> for Badge {
    fn from(u: usize) -> Self {
        let shifted_left = u << 4;
        Badge {
            inner: shifted_left >> 4,
        }
    }
}

impl From<Badge> for usize {
    fn from(b: Badge) -> Self {
        b.inner
    }
}
