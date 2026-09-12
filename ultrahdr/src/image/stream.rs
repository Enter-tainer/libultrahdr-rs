//! Compressed streams: encoder inputs, encoder outputs and borrowed metadata blocks.

use crate::image::aspects::ColorAspects;
use crate::sys;
use std::ffi::c_void;

/// A compressed JPEG image, either as encoder input or the result of encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedImage {
    data: Vec<u8>,
    aspects: ColorAspects,
}

impl CompressedImage {
    /// Wrap an encoded stream with unspecified colour aspects.
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            aspects: ColorAspects::UNSPECIFIED,
        }
    }

    /// Wrap an encoded stream together with its colour description.
    pub fn with_aspects(data: impl Into<Vec<u8>>, aspects: ColorAspects) -> Self {
        Self {
            data: data.into(),
            aspects,
        }
    }

    /// The encoded bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Number of encoded bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Colour description of the stream.
    pub fn aspects(&self) -> ColorAspects {
        self.aspects
    }

    /// Replace the colour description.
    pub fn set_aspects(&mut self, aspects: ColorAspects) {
        self.aspects = aspects;
    }

    /// Build the C descriptor. The library copies the stream on registration.
    pub(crate) fn as_sys(&self) -> sys::uhdr_compressed_image {
        let (cg, ct, range) = self.aspects.to_sys();
        sys::uhdr_compressed_image {
            data: self.data.as_ptr() as *mut c_void,
            data_sz: self.data.len(),
            capacity: self.data.len(),
            cg,
            ct,
            range,
        }
    }
}

/// Owned encoded stream returned by [`Encoder::encoded_stream`](crate::Encoder::encoded_stream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage {
    /// The encoded bytes.
    pub data: Vec<u8>,
    /// Colour description attached to the stream.
    pub aspects: ColorAspects,
}

/// Borrowed view over an encoded stream owned by an [`Encoder`](crate::Encoder).
#[derive(Debug)]
pub struct EncodedView<'a> {
    image: &'a mut sys::uhdr_compressed_image,
}

// SAFETY: the view holds an exclusive borrow of the encoder, so no other access to the stream can
// exist while it is alive; the stream itself is plain memory with no thread affinity.
unsafe impl Send for EncodedView<'_> {}

impl<'a> EncodedView<'a> {
    pub(crate) fn new(image: &'a mut sys::uhdr_compressed_image) -> Self {
        Self { image }
    }

    /// The encoded bytes.
    pub fn bytes(&self) -> &'a [u8] {
        // SAFETY: libultrahdr owns the buffer for as long as the codec instance lives, and the
        // view borrows the codec.
        unsafe { std::slice::from_raw_parts(self.image.data as *const u8, self.image.data_sz) }
    }

    /// Colour description of the stream.
    pub fn aspects(&self) -> ColorAspects {
        ColorAspects::from_sys(self.image.cg, self.image.ct, self.image.range)
    }

    /// Copy the stream out of the codec.
    pub fn to_owned_image(&self) -> EncodedImage {
        EncodedImage {
            data: self.bytes().to_vec(),
            aspects: self.aspects(),
        }
    }
}

/// Borrowed view over a length-delimited byte block owned by libultrahdr.
///
/// Used for the EXIF payload, the ICC profile and the compressed parts of a decoded stream.
/// Several views may be held at once (for example the base image and the gain map of one stream);
/// they all borrow the [`ProbedDecoder`](crate::ProbedDecoder) that produced them.
#[derive(Debug)]
pub struct MemBlockView<'a> {
    data: &'a [u8],
    capacity: usize,
}

impl<'a> MemBlockView<'a> {
    pub(crate) fn new(data: *const u8, len: usize, capacity: usize) -> Result<Self, crate::Error> {
        use crate::error::Error;

        if data.is_null() {
            return Err(Error::invalid_parameter("null data pointer"));
        }
        if len > capacity {
            return Err(Error::invalid_parameter(format!(
                "length {len} exceeds capacity {capacity}"
            )));
        }
        // SAFETY: the caller guarantees `data..data + len` is readable for the borrow.
        Ok(Self {
            data: unsafe { std::slice::from_raw_parts(data, len) },
            capacity,
        })
    }

    /// Size of the allocation backing the block, in bytes; always `>= len()`.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Borrowed bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the block is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Copy the bytes out of libultrahdr's buffer.
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_block_view_validates_bounds() {
        let data = [1u8, 2, 3, 4];
        let block = MemBlockView::new(data.as_ptr(), 4, 4).unwrap();
        assert_eq!(block.bytes(), &data);
        assert_eq!(block.len(), 4);
        assert!(!block.is_empty());
        assert_eq!(block.to_vec(), data.to_vec());

        assert!(MemBlockView::new(std::ptr::null(), 0, 0).is_err());
        assert!(MemBlockView::new(data.as_ptr(), 5, 4).is_err());
        let empty = MemBlockView::new(data.as_ptr(), 0, 4).unwrap();
        assert!(empty.is_empty());
    }
}
