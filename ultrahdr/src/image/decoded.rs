//! Decoded pixels: owned copies and borrowed views over the decoder's buffers.

use crate::error::{Error, Result};
use crate::image::aspects::ColorAspects;
use crate::image::format::{PixelFormat, Plane};
use crate::image::raw::RawImage;
use crate::sys;

/// Owned packed pixels returned by [`Decoder`](crate::Decoder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Pixel layout, always one of the packed formats.
    pub format: PixelFormat,
    /// Colour description of the decoded pixels.
    pub aspects: ColorAspects,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Tightly packed pixels, `width * height * bytes_per_pixel` bytes long.
    pub data: Vec<u8>,
}

impl DecodedImage {
    /// Iterate over the rows of the image, without padding.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        let row_bytes = (self.width as usize)
            * self
                .format
                .bytes_per_pixel()
                .expect("decoded images are packed");
        self.data.chunks_exact(row_bytes)
    }

    /// Re-wrap the pixels as a [`RawImage`], e.g. to feed them to an encoder.
    ///
    /// The pixels are moved, not copied. Aspects the stream did not signal stay `None`, which the
    /// encoder rejects for raw input, so fill them in first (see [`ColorAspects`]).
    pub fn into_raw_image(self) -> Result<RawImage> {
        let Self {
            format,
            aspects,
            width,
            height,
            data,
        } = self;
        RawImage::from_packed(format, width, height, data, aspects)
    }
}

/// Borrowed view over packed pixels owned by a [`Decoder`](crate::Decoder).
#[derive(Debug)]
pub struct DecodedView<'a> {
    image: &'a mut sys::uhdr_raw_image,
    format: PixelFormat,
    bpp: usize,
}

// SAFETY: the view holds an exclusive borrow of the decoder, so no other access to the pixels can
// exist while it is alive; the pixels themselves are plain memory with no thread affinity.
unsafe impl Send for DecodedView<'_> {}

impl<'a> DecodedView<'a> {
    pub(crate) fn new(image: &'a mut sys::uhdr_raw_image) -> Result<Self> {
        let format = PixelFormat::try_from(image.fmt)?;
        let bpp = format
            .bytes_per_pixel()
            .ok_or_else(|| Error::invalid_parameter("decoded images must be packed"))?;
        Ok(Self { image, format, bpp })
    }

    /// Logical width in pixels.
    pub fn width(&self) -> u32 {
        self.image.w
    }

    /// Logical height in pixels.
    pub fn height(&self) -> u32 {
        self.image.h
    }

    /// Pixel layout of the decoded pixels.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Colour description of the decoded pixels.
    pub fn aspects(&self) -> ColorAspects {
        ColorAspects::from_sys(self.image.cg, self.image.ct, self.image.range)
    }

    /// Replace the colour description of the decoded pixels.
    ///
    /// Useful when a stream does not signal its colour aspects and the caller knows them from
    /// elsewhere (an ICC profile, for example).
    pub fn set_aspects(&mut self, aspects: ColorAspects) {
        let (cg, ct, range) = aspects.to_sys();
        self.image.cg = cg;
        self.image.ct = ct;
        self.image.range = range;
    }

    /// The C descriptor, for handing the pixels to an encoder without copying them.
    pub(crate) fn as_sys(&self) -> sys::uhdr_raw_image {
        *self.image
    }

    /// Borrow a single row, without any row padding.
    pub fn row(&self, y: usize) -> Result<&'a [u8]> {
        if y as u32 >= self.image.h {
            return Err(Error::invalid_parameter(format!(
                "row {y} is outside the image (height {})",
                self.image.h
            )));
        }
        let stride = self.image.stride[Plane::Y.index()] as usize;
        let start = y * stride * self.bpp;
        let end = start + self.width() as usize * self.bpp;
        let plane = self.image.planes[Plane::Y.index()] as *const u8;
        // SAFETY: the packed plane is `stride * height * bpp` bytes (libultrahdr guarantees
        // `stride >= width`), so `start..end` is in bounds for a valid row index.
        Ok(unsafe { std::slice::from_raw_parts(plane.add(start), end - start) })
    }

    /// Iterate over the rows of the decoded image, without row padding.
    ///
    /// The iterator yields one slice per row; it cannot fail, because it only visits rows that
    /// exist.
    pub fn rows(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        (0..self.height() as usize).map(move |y| {
            // PANIC SAFETY: `y < height`, which `row` accepts.
            self.row(y).expect("row index is within the image")
        })
    }

    /// Copy the pixels out of the codec into a tightly packed buffer.
    pub fn to_owned_image(&self) -> DecodedImage {
        let width = self.width() as usize;
        let height = self.height() as usize;
        let bpp = self.bpp;
        let stride = self.image.stride[Plane::Y.index()] as usize;
        let plane = self.image.planes[Plane::Y.index()] as *const u8;
        // SAFETY: as in `row`, the plane holds `stride * height * bpp` readable bytes.
        let source = unsafe { std::slice::from_raw_parts(plane, stride * height * bpp) };
        let mut data = Vec::with_capacity(width * height * bpp);
        for y in 0..height {
            let start = y * stride * bpp;
            data.extend_from_slice(&source[start..start + width * bpp]);
        }
        DecodedImage {
            format: self.format,
            aspects: self.aspects(),
            width: width as u32,
            height: height as u32,
            data,
        }
    }
}

/// The decoded base image and its gain map, borrowed together from one decode call.
///
/// Produced by [`ProbedDecoder::decode_with_gainmap`](crate::ProbedDecoder::decode_with_gainmap).
/// Both views borrow the decoder, so they can be inspected side by side — unlike separate calls
/// to `decode` and `decoded_gainmap`, which cannot be outstanding at the same time.
#[derive(Debug)]
pub struct DecodedFrame<'a> {
    /// The decoded base image.
    pub image: DecodedView<'a>,
    /// The decoded gain map, if the stream carries one.
    pub gainmap: Option<DecodedView<'a>>,
}

// SAFETY: both views hold an exclusive borrow of the decoder, so no other access to the decoded
// buffers can exist while the frame is alive.
unsafe impl Send for DecodedFrame<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::aspects::{ColorGamut, ColorRange, ColorTransfer};

    const ASPECTS: ColorAspects =
        ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);

    #[test]
    fn decoded_image_round_trips_into_a_raw_image() {
        let decoded = DecodedImage {
            format: PixelFormat::Rgba8888,
            aspects: ASPECTS,
            width: 2,
            height: 2,
            data: vec![7u8; 16],
        };
        let raw = decoded.into_raw_image().unwrap();
        assert_eq!(raw.width(), 2);
        assert_eq!(raw.format(), PixelFormat::Rgba8888);
        assert_eq!(raw.data().unwrap(), &[7u8; 16]);

        let broken = DecodedImage {
            format: PixelFormat::Rgba8888,
            aspects: ASPECTS,
            width: 2,
            height: 2,
            data: vec![0u8; 15],
        };
        assert!(matches!(
            broken.into_raw_image(),
            Err(Error::InvalidParameter(_))
        ));
    }
}
