// Modifications copyright (c) 2026, Jiaqi (0x7c13) Liu. MIT license.
// Shared decoder storage policy. There is one PNG state machine.
use super::stream::DecodingError;
#[cfg(feature = "notepads-markdown-resources")]
use markdown_svg_budget::{BudgetVec, ResourceKind, ResourcesRef};

#[derive(Clone, Default)]
pub(crate) struct Control {
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) resources: Option<ResourcesRef>,
}
impl Control {
    pub(crate) fn checkpoint(&self, work: usize) -> Result<(), DecodingError> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(r) = &self.resources {
            r.checkpoint(work as u64)
                .map_err(DecodingError::Resources)?;
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        let _ = work;
        Ok(())
    }
    pub(crate) fn guard<T>(&self, result: Result<T, DecodingError>) -> Result<T, DecodingError> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(r) = &self.resources {
            return result.map_err(|error| {
                let fixed = match error {
                    DecodingError::Resources(e) => e,
                    DecodingError::LimitsExceeded => markdown_svg_budget::Error::new(
                        markdown_svg_budget::FailureKind::Quota,
                        markdown_svg_budget::LimitKind::Scratch,
                    ),
                    _ => markdown_svg_budget::Error::new(
                        markdown_svg_budget::FailureKind::ImageDecode,
                        markdown_svg_budget::LimitKind::None,
                    ),
                };
                DecodingError::Resources(r.record(fixed))
            });
        }
        result
    }
    pub(crate) fn is_paid(&self) -> bool {
        #[cfg(feature = "notepads-markdown-resources")]
        {
            self.resources.is_some()
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        {
            false
        }
    }
}
#[derive(Debug)]
pub(crate) enum Buffer<T> {
    Plain(Vec<T>),
    #[cfg(feature = "notepads-markdown-resources")]
    Paid(BudgetVec<T>, ResourcesRef),
}
impl<T> Default for Buffer<T> {
    fn default() -> Self {
        Self::Plain(Vec::new())
    }
}
impl<T> Buffer<T> {
    pub(crate) fn new(control: &Control) -> Self {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(r) = &control.resources {
            return Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch), r.clone());
        }
        let _ = control;
        Self::default()
    }
    pub(crate) fn reserve_exact(&mut self, n: usize) -> Result<(), DecodingError> {
        match self {
            Self::Plain(v) => v.reserve_exact(n),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, _) => v.try_reserve_exact(n).map_err(DecodingError::Resources)?,
        }
        Ok(())
    }
    pub(crate) fn capacity(&self) -> usize {
        match self {
            Self::Plain(v) => v.capacity(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, _) => v.capacity(),
        }
    }
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Plain(v) => v.clear(),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, _) => v.clear(),
        }
    }
    pub(crate) fn truncate(&mut self, len: usize) {
        match self {
            Self::Plain(v) => v.truncate(len),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, _) => {
                while v.len() > len {
                    v.pop();
                }
            }
        }
    }
    pub(crate) fn resize(&mut self, len: usize, value: T) -> Result<(), DecodingError>
    where
        T: Clone,
    {
        match self {
            Self::Plain(v) => v.resize(len, value),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, r) => {
                if len < v.len() {
                    while v.len() > len {
                        v.pop();
                    }
                } else {
                    v.try_reserve_exact(len - v.len())
                        .map_err(DecodingError::Resources)?;
                    while v.len() < len {
                        let end =
                            (v.len() + (4096 / std::mem::size_of::<T>().max(1)).max(1)).min(len);
                        r.checkpoint((end - v.len()) as u64)
                            .map_err(DecodingError::Resources)?;
                        while v.len() < end {
                            v.try_push(value.clone())
                                .map_err(DecodingError::Resources)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn extend_from_slice(&mut self, bytes: &[T]) -> Result<(), DecodingError>
    where
        T: Clone,
    {
        match self {
            Self::Plain(v) => v.extend_from_slice(bytes),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, r) => {
                v.try_reserve(bytes.len())
                    .map_err(DecodingError::Resources)?;
                for part in bytes.chunks((4096 / std::mem::size_of::<T>().max(1)).max(1)) {
                    r.checkpoint(part.len() as u64)
                        .map_err(DecodingError::Resources)?;
                    v.try_extend_from_slice(part)
                        .map_err(DecodingError::Resources)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn copy(&self, control: &Control) -> Result<Self, DecodingError>
    where
        T: Clone,
    {
        let mut v = Self::new(control);
        v.reserve_exact(self.len())?;
        v.extend_from_slice(self)?;
        Ok(v)
    }
    pub(crate) fn into_plain(self) -> Vec<T> {
        match self {
            Self::Plain(v) => v,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(_, _) => unreachable!("paid storage cannot escape as Vec"),
        }
    }
    pub(crate) fn as_slice(&self) -> &[T] {
        self
    }
}
impl<T> std::ops::Deref for Buffer<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(v) => v,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, _) => v,
        }
    }
}
impl<T> std::ops::DerefMut for Buffer<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(v) => v,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v, _) => v,
        }
    }
}

pub(crate) enum One<T> {
    Plain(Box<T>),
    #[cfg(feature = "notepads-markdown-resources")]
    Paid(BudgetVec<T>),
}
impl<T> One<T> {
    pub(crate) fn new(
        control: &Control,
        make: impl FnOnce() -> Result<T, DecodingError>,
    ) -> Result<Self, DecodingError> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(r) = &control.resources {
            let mut v = BudgetVec::new(r.clone(), ResourceKind::Scratch);
            v.try_reserve_exact(1).map_err(DecodingError::Resources)?;
            v.try_push(make()?).map_err(DecodingError::Resources)?;
            return Ok(Self::Paid(v));
        }
        let _ = control;
        Ok(Self::Plain(Box::new(make()?)))
    }
}
impl<T> std::ops::Deref for One<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Self::Plain(v) => v,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v) => &v[0],
        }
    }
}
impl<T> std::ops::DerefMut for One<T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            Self::Plain(v) => v,
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(v) => &mut v[0],
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Field {
    Sbit,
    Trns,
    Palette,
    Bkgd,
    Icc,
    Exif,
}
pub(crate) enum Metadata {
    Plain,
    #[cfg(feature = "notepads-markdown-resources")]
    Paid(PaidMetadata),
}
#[cfg(feature = "notepads-markdown-resources")]
pub(crate) struct PaidMetadata {
    bytes: [Option<Buffer<u8>>; 6],
    // Validated retained text chunks keep their original encoded bytes. They
    // are private, never exposed as legacy Strings or lazy unbudgeted getters.
    pub(crate) texts: BudgetVec<Buffer<u8>>,
}
impl Metadata {
    pub(crate) fn new(control: &Control) -> Self {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(r) = &control.resources {
            return Self::Paid(PaidMetadata {
                bytes: std::array::from_fn(|_| None),
                texts: BudgetVec::new(r.clone(), ResourceKind::Scratch),
            });
        }
        let _ = control;
        Self::Plain
    }
    pub(crate) fn get<'a>(&'a self, info: &'a crate::Info<'_>, field: Field) -> Option<&'a [u8]> {
        match self {
            Self::Plain => match field {
                Field::Sbit => info.sbit.as_deref(),
                Field::Trns => info.trns.as_deref(),
                Field::Palette => info.palette.as_deref(),
                Field::Bkgd => info.bkgd.as_deref(),
                Field::Icc => info.icc_profile.as_deref(),
                Field::Exif => info.exif_metadata.as_deref(),
            },
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(p) => p.bytes[field as usize].as_deref(),
        }
    }
    pub(crate) fn set(&mut self, info: &mut crate::Info<'static>, field: Field, value: Buffer<u8>) {
        match self {
            Self::Plain => {
                let value = Some(std::borrow::Cow::Owned(value.into_plain()));
                match field {
                    Field::Sbit => info.sbit = value,
                    Field::Trns => info.trns = value,
                    Field::Palette => info.palette = value,
                    Field::Bkgd => info.bkgd = value,
                    Field::Icc => info.icc_profile = value,
                    Field::Exif => info.exif_metadata = value,
                }
            }
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(p) => p.bytes[field as usize] = Some(value),
        }
    }
    pub(crate) fn view<'a>(
        &'a self,
        info: &'a crate::Info<'static>,
    ) -> std::borrow::Cow<'a, crate::Info<'a>> {
        match self {
            Self::Plain => std::borrow::Cow::Borrowed(info),
            #[cfg(feature = "notepads-markdown-resources")]
            Self::Paid(_) => {
                // Scalar Info contains no owned metadata in this branch.
                let mut view = info.clone();
                view.sbit = self.get(info, Field::Sbit).map(std::borrow::Cow::Borrowed);
                view.trns = self.get(info, Field::Trns).map(std::borrow::Cow::Borrowed);
                view.palette = self
                    .get(info, Field::Palette)
                    .map(std::borrow::Cow::Borrowed);
                view.bkgd = self.get(info, Field::Bkgd).map(std::borrow::Cow::Borrowed);
                view.icc_profile = self.get(info, Field::Icc).map(std::borrow::Cow::Borrowed);
                view.exif_metadata = self.get(info, Field::Exif).map(std::borrow::Cow::Borrowed);
                std::borrow::Cow::Owned(view)
            }
        }
    }
}

// Check every actual bounded batch, shared by the inner pixel kernels.
pub(crate) struct Work<'a> {
    control: &'a Control,
    pending: usize,
}
impl<'a> Work<'a> {
    pub(crate) fn new(control: &'a Control) -> Result<Self, DecodingError> {
        control.checkpoint(0)?;
        Ok(Self {
            control,
            pending: 0,
        })
    }
    pub(crate) fn step(&mut self, n: usize) -> Result<(), DecodingError> {
        self.pending += n;
        if self.pending >= 4096 {
            self.control.checkpoint(self.pending)?;
            self.pending = 0;
        }
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<(), DecodingError> {
        self.control.checkpoint(self.pending)
    }
}
pub(crate) fn find_null(input: &[u8], control: &Control) -> Result<Option<usize>, DecodingError> {
    for (index, part) in input.chunks(4096).enumerate() {
        control.checkpoint(part.len())?;
        if let Some(offset) = part.iter().position(|&b| b == 0) {
            return Ok(Some(index * 4096 + offset));
        }
    }
    Ok(None)
}
pub(crate) fn valid_ascii(input: &[u8], control: &Control) -> Result<bool, DecodingError> {
    for part in input.chunks(4096) {
        control.checkpoint(part.len())?;
        if !part.is_ascii() {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(crate) fn valid_utf8(mut input: &[u8], control: &Control) -> Result<bool, DecodingError> {
    while !input.is_empty() {
        let end = input.len().min(4096);
        control.checkpoint(end)?;
        match std::str::from_utf8(&input[..end]) {
            Ok(_) => input = &input[end..],
            Err(error) if error.error_len().is_none() && end < input.len() => {
                let good = error.valid_up_to();
                if good == 0 {
                    return Ok(false);
                }
                input = &input[good..];
            }
            Err(_) => return Ok(false),
        }
    }
    Ok(true)
}
