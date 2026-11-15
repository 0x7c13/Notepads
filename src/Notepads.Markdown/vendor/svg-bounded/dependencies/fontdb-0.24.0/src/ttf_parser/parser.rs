//! Binary parsing utils.

use core::convert::TryInto;

/// A trait for parsing raw binary data of fixed size.
///
/// This is a low-level, internal trait that should not be used directly.
pub trait FromData: Sized {
    /// Object's raw data size.
    ///
    /// Not always the same as `mem::size_of`.
    const SIZE: usize;

    /// Parses an object from a raw data.
    fn parse(data: &[u8]) -> Option<Self>;
}

impl FromData for u16 {
    const SIZE: usize = 2;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        data.try_into().ok().map(u16::from_be_bytes)
    }
}

impl FromData for u32 {
    const SIZE: usize = 4;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        data.try_into().ok().map(u32::from_be_bytes)
    }
}

/// A safe u32 to usize casting.
///
/// Rust doesn't implement `From<u32> for usize`,
/// because it has to support 16 bit targets.
/// We don't, so we can allow this.
pub trait NumFrom<T>: Sized {
    /// Converts u32 into usize.
    fn num_from(_: T) -> Self;
}

impl NumFrom<u32> for usize {
    #[inline]
    fn num_from(v: u32) -> Self {
        #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
        {
            v as usize
        }

        // compilation error on 16 bit targets
    }
}

/// A slice-like container that converts internal binary data only on access.
///
/// Array values are stored in a continuous data chunk.
#[derive(Clone, Copy)]
pub struct LazyArray16<'a, T> {
    data: &'a [u8],
    data_type: core::marker::PhantomData<T>,
}

impl<T> Default for LazyArray16<'_, T> {
    #[inline]
    fn default() -> Self {
        LazyArray16 {
            data: &[],
            data_type: core::marker::PhantomData,
        }
    }
}

impl<'a, T: FromData> LazyArray16<'a, T> {
    /// Creates a new `LazyArray`.
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        LazyArray16 {
            data,
            data_type: core::marker::PhantomData,
        }
    }

    /// Returns a value at `index`.
    #[inline]
    pub fn get(&self, index: u16) -> Option<T> {
        if index < self.len() {
            let start = usize::from(index) * T::SIZE;
            let end = start + T::SIZE;
            self.data.get(start..end).and_then(T::parse)
        } else {
            None
        }
    }

    /// Returns array's length.
    #[inline]
    pub fn len(&self) -> u16 {
        (self.data.len() / T::SIZE) as u16
    }

    /// Performs a binary search using specified closure.
    #[inline]
    pub fn binary_search_by<F>(&self, mut f: F) -> Option<(u16, T)>
    where
        F: FnMut(&T) -> core::cmp::Ordering,
    {
        // Based on Rust std implementation.

        use core::cmp::Ordering;

        let mut size = self.len();
        if size == 0 {
            return None;
        }

        let mut base = 0;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            // mid is always in [0, size), that means mid is >= 0 and < size.
            // mid >= 0: by definition
            // mid < size: mid = size / 2 + size / 4 + size / 8 ...
            let cmp = f(&self.get(mid)?);
            base = if cmp == Ordering::Greater { base } else { mid };
            size -= half;
        }

        // base is always in [0, size) because base <= mid.
        let value = self.get(base)?;
        if f(&value) == Ordering::Equal {
            Some((base, value))
        } else {
            None
        }
    }
}

impl<'a, T: FromData> IntoIterator for LazyArray16<'a, T> {
    type Item = T;
    type IntoIter = LazyArrayIter16<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        LazyArrayIter16 {
            data: self,
            index: 0,
        }
    }
}

/// An iterator over `LazyArray16`.
#[derive(Clone, Copy)]
pub struct LazyArrayIter16<'a, T> {
    data: LazyArray16<'a, T>,
    index: u16,
}

impl<'a, T: FromData> Iterator for LazyArrayIter16<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.index += 1; // TODO: check
        self.data.get(self.index - 1)
    }

    #[inline]
    fn count(self) -> usize {
        usize::from(self.data.len().saturating_sub(self.index))
    }
}

/// A slice-like container that converts internal binary data only on access.
///
/// This is a low-level, internal structure that should not be used directly.
#[derive(Clone, Copy)]
pub struct LazyArray32<'a, T> {
    data: &'a [u8],
    data_type: core::marker::PhantomData<T>,
}

impl<'a, T: FromData> LazyArray32<'a, T> {
    /// Creates a new `LazyArray`.
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        LazyArray32 {
            data,
            data_type: core::marker::PhantomData,
        }
    }

    /// Returns a value at `index`.
    #[inline]
    pub fn get(&self, index: u32) -> Option<T> {
        if index < self.len() {
            let start = usize::num_from(index) * T::SIZE;
            let end = start + T::SIZE;
            self.data.get(start..end).and_then(T::parse)
        } else {
            None
        }
    }

    /// Returns array's length.
    #[inline]
    pub fn len(&self) -> u32 {
        (self.data.len() / T::SIZE) as u32
    }
}

/// A streaming binary parser.
#[derive(Clone, Default, Debug)]
pub struct Stream<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Stream<'a> {
    /// Creates a new `Stream` parser.
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Stream { data, offset: 0 }
    }

    /// Returns the current offset.
    #[inline]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the trailing data.
    ///
    /// Returns `None` when `Stream` is reached the end.
    #[inline]
    pub fn tail(&self) -> Option<&'a [u8]> {
        self.data.get(self.offset..)
    }

    /// Advances by `FromData::SIZE`.
    ///
    /// Doesn't check bounds.
    #[inline]
    pub fn skip<T: FromData>(&mut self) {
        self.advance(T::SIZE);
    }

    /// Advances by the specified `len`.
    ///
    /// Doesn't check bounds.
    #[inline]
    pub fn advance(&mut self, len: usize) {
        self.offset += len;
    }

    /// Advances by the specified `len` and checks for bounds.
    #[inline]
    pub fn advance_checked(&mut self, len: usize) -> Option<()> {
        if self.offset + len <= self.data.len() {
            self.advance(len);
            Some(())
        } else {
            None
        }
    }

    /// Parses the type from the steam.
    ///
    /// Returns `None` when there is not enough data left in the stream
    /// or the type parsing failed.
    #[inline]
    pub fn read<T: FromData>(&mut self) -> Option<T> {
        self.read_bytes(T::SIZE).and_then(T::parse)
    }

    /// Parses the type from the steam at offset.
    #[inline]
    pub fn read_at<T: FromData>(data: &[u8], offset: usize) -> Option<T> {
        data.get(offset..offset + T::SIZE).and_then(T::parse)
    }

    /// Reads N bytes from the stream.
    #[inline]
    pub fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        // An integer overflow here on 32bit systems is almost guarantee to be caused
        // by an incorrect parsing logic from the caller side.
        // Simply using `checked_add` here would silently swallow errors, which is not what we want.
        debug_assert!(self.offset as u64 + len as u64 <= u32::MAX as u64);

        let v = self.data.get(self.offset..self.offset + len)?;
        self.advance(len);
        Some(v)
    }

    /// Reads the next `count` types as a slice.
    #[inline]
    pub fn read_array16<T: FromData>(&mut self, count: u16) -> Option<LazyArray16<'a, T>> {
        let len = usize::from(count) * T::SIZE;
        self.read_bytes(len).map(LazyArray16::new)
    }

    /// Reads the next `count` types as a slice.
    #[inline]
    pub fn read_array32<T: FromData>(&mut self, count: u32) -> Option<LazyArray32<'a, T>> {
        let len = usize::num_from(count) * T::SIZE;
        self.read_bytes(len).map(LazyArray32::new)
    }
}

/// A common offset methods.
pub trait Offset {
    /// Converts the offset to `usize`.
    fn to_usize(&self) -> usize;
}

/// A type-safe u16 offset.
#[derive(Clone, Copy, Debug)]
pub struct Offset16(pub u16);

impl Offset for Offset16 {
    #[inline]
    fn to_usize(&self) -> usize {
        usize::from(self.0)
    }
}

impl FromData for Offset16 {
    const SIZE: usize = 2;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        u16::parse(data).map(Offset16)
    }
}

/// A type-safe u32 offset.
#[derive(Clone, Copy, Debug)]
pub struct Offset32(pub u32);

impl Offset for Offset32 {
    #[inline]
    fn to_usize(&self) -> usize {
        usize::num_from(self.0)
    }
}

impl FromData for Offset32 {
    const SIZE: usize = 4;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        u32::parse(data).map(Offset32)
    }
}
