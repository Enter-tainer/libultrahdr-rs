//! UltraHDR decoder.
//!
//! # State machine
//!
//! libultrahdr's decoder has a strict lifecycle: it accepts configuration until it is probed, and
//! probing freezes the input image and the output settings. This wrapper encodes that lifecycle in
//! the *types*: a [`Decoder`] is configurable, and [`Decoder::probe`] consumes it and returns a
//! [`ProbedDecoder`] whose configuration is frozen. Calling a configuration setter on a probed
//! decoder is a compile-time error instead of a runtime one.
//!
//! The split has a second benefit: every getter on [`ProbedDecoder`] takes `&self`, so the EXIF
//! payload, the ICC profile, the compressed base image and the compressed gain map can be
//! borrowed **at the same time**. Decoding still takes `&mut self`, because it mutates the
//! decoder; use [`ProbedDecoder::decode_with_gainmap`] to borrow the decoded image and the gain
//! map together.
//!
//! ```no_run
//! use ultrahdr::{CompressedImage, DecodedOutput, Decoder};
//!
//! # fn main() -> ultrahdr::Result<()> {
//! # let bytes: Vec<u8> = Vec::new();
//! let mut dec = Decoder::new()?;
//! dec.set_image(&CompressedImage::new(bytes))?;
//! // Settings first, then probing.
//! dec.set_output(DecodedOutput::Pq1010102)?;
//!
//! let mut dec = dec.probe()?;
//! let info = dec.info()?;
//! println!("{}x{} gain map: {:?}", info.width, info.height, info.gainmap_size);
//!
//! let pixels = dec.decode()?;
//! assert_eq!(pixels.width(), info.width);
//! # Ok(())
//! # }
//! ```

use crate::GainMapMetadata;
use crate::edit;
use crate::edit::{CropRect, Mirror, Rotation};
use crate::error::{Error, Result, check};
use crate::image::{CompressedImage, DecodedFrame, DecodedOutput, DecodedView, MemBlockView};
use crate::sys;
use std::ptr::NonNull;

/// Check whether `data` is a valid UltraHDR stream, i.e. whether it carries a primary image, a gain
/// map image and gain map metadata.
///
/// This is a cheap header probe: no pixels are decoded. `false` is also returned for buffers
/// shorter than 4 bytes or longer than `i32::MAX`.
pub fn is_uhdr_image(data: &[u8]) -> bool {
    if data.len() < 4 || data.len() > i32::MAX as usize {
        return false;
    }
    // SAFETY: the upstream implementation only parses the buffer and never writes to it, nor does
    // it retain the pointer past the call.
    unsafe { sys::is_uhdr_image(data.as_ptr() as *mut _, data.len() as i32) != 0 }
}

/// Information about a stream, as reported by [`ProbedDecoder::info`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    /// Width of the base image in pixels.
    pub width: u32,
    /// Height of the base image in pixels.
    pub height: u32,
    /// Width and height of the gain map, or `None` if the stream carries no gain map.
    pub gainmap_size: Option<(u32, u32)>,
    /// Gain map metadata, if the stream carries any.
    pub metadata: Option<GainMapMetadata>,
}

/// Configurable UltraHDR decoder, before the stream is probed.
///
/// Owns the underlying `uhdr_codec_private_t`. Register an image, configure the output, then turn
/// it into a [`ProbedDecoder`] with [`probe`](Self::probe). Effects (crop, mirror, ...) may also
/// be added here; libultrahdr applies them when decoding and accepts them until then.
///
/// If [`probe`](Self::probe) fails, the decoder is dropped: the C context stays locked after a
/// failed probe, so a fresh [`Decoder::new`] is the only way to try another stream.
#[derive(Debug)]
pub struct Decoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
    /// An image has been registered via [`Decoder::set_image`].
    image_set: bool,
}

// SAFETY: a libultrahdr decoder is a self-contained C++ object: it keeps no thread-local state and
// registers nothing per-thread (verified against the upstream sources, which contain no
// `thread_local`/`__thread`/`pthread_key` usage). Moving the handle to another thread and using
// it exclusively there is therefore sound. `Sync` is deliberately NOT implemented: the C API has
// no internal synchronization, so concurrent calls through a shared reference would race.
unsafe impl Send for Decoder {}

impl Decoder {
    /// Create a new decoder instance.
    pub fn new() -> Result<Self> {
        // SAFETY: create returns an owned handle or null.
        let raw =
            NonNull::new(unsafe { sys::uhdr_create_decoder() }).ok_or_else(Error::allocation)?;
        Ok(Self {
            raw,
            image_set: false,
        })
    }

    /// Register the compressed image to decode.
    ///
    /// libultrahdr copies the stream, so `image` may be dropped as soon as this returns.
    /// Re-registering replaces the previous image. An empty stream is rejected eagerly: its
    /// dangling data pointer would pass the C null check and crash the parser.
    pub fn set_image(&mut self, image: &CompressedImage) -> Result<()> {
        if image.is_empty() {
            return Err(Error::invalid_parameter(
                "compressed image holds no bytes; a JPEG stream is required",
            ));
        }
        let mut raw = image.as_sys();
        // SAFETY: the library copies the stream during the call.
        check(unsafe { sys::uhdr_dec_set_image(self.raw.as_ptr(), &mut raw) })?;
        self.image_set = true;
        Ok(())
    }

    /// Choose the decoded output profile: pixel format and transfer function together.
    ///
    /// libultrahdr only accepts four pairings of format and transfer; [`DecodedOutput`] enumerates
    /// exactly those, so an invalid pairing cannot be expressed. The default is
    /// [`DecodedOutput::LinearF16`], matching `uhdr_reset_decoder`.
    pub fn set_output(&mut self, output: DecodedOutput) -> Result<()> {
        // SAFETY: the handle is valid and owned by `self`; the format comes from a
        // `DecodedOutput`, so it is one of the three the C API accepts.
        check(unsafe {
            sys::uhdr_dec_set_out_img_format(self.raw.as_ptr(), output.format().to_sys())
        })?;
        // SAFETY: the handle is valid and owned by `self`; the transfer comes from a
        // `DecodedOutput`, so it is one of the four the C API accepts.
        check(unsafe {
            sys::uhdr_dec_set_out_color_transfer(self.raw.as_ptr(), output.transfer().to_sys())
        })
    }

    /// Clamp the maximum display boost applied when reconstructing HDR.
    ///
    /// Values below 1.0 (and non-finite values) are rejected by libultrahdr.
    pub fn set_max_display_boost(&mut self, boost: f32) -> Result<()> {
        // SAFETY: the handle is valid and owned by `self`.
        check(unsafe { sys::uhdr_dec_set_out_max_display_boost(self.raw.as_ptr(), boost) })
    }

    /// Parse the stream headers, gain map metadata and embedded blocks, freezing the
    /// configuration.
    ///
    /// This consumes the decoder and returns a [`ProbedDecoder`], whose getters read the parsed
    /// information. If parsing fails, the decoder is dropped (see the type-level documentation).
    pub fn probe(self) -> Result<ProbedDecoder> {
        self.probe_impl()
    }

    /// Configure the output and probe in one call.
    ///
    /// Equivalent to [`set_output`](Self::set_output) followed by [`probe`](Self::probe).
    pub fn probe_as(mut self, output: DecodedOutput) -> Result<ProbedDecoder> {
        self.set_output(output)?;
        self.probe_impl()
    }

    /// Enable/disable GPU acceleration. A no-op unless libultrahdr was built with GLES support.
    pub fn enable_gpu_acceleration(&mut self, enable: bool) -> Result<()> {
        edit::enable_gpu_acceleration(self.raw, enable)
    }

    /// Mirror the decoded image. Effects apply in the order they are added, after decoding.
    pub fn mirror(&mut self, direction: Mirror) -> Result<()> {
        edit::add_effect_mirror(self.raw, direction)
    }

    /// Rotate the decoded image clockwise.
    pub fn rotate(&mut self, rotation: Rotation) -> Result<()> {
        edit::add_effect_rotate(self.raw, rotation)
    }

    /// Crop the decoded image to `rect`, given in absolute, exclusive pixel coordinates.
    pub fn crop(&mut self, rect: CropRect) -> Result<()> {
        edit::add_effect_crop(self.raw, rect)
    }

    /// Resize the decoded image.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        edit::add_effect_resize(self.raw, width, height)
    }

    /// Shared tail of [`probe`](Self::probe) and [`probe_as`](Self::probe_as).
    fn probe_impl(self) -> Result<ProbedDecoder> {
        if !self.image_set {
            return Err(Error::invalid_operation(
                "decoder has no image; call set_image before probing",
            ));
        }
        // Move the handle out of `self` (a `Drop` type) into the probed decoder without running
        // `Decoder::drop`, which would release it.
        let raw = self.raw;
        std::mem::forget(self);
        let probed = ProbedDecoder { raw };
        // The C context locks itself as soon as probe runs, even if parsing then fails; on error
        // `probed` is dropped by the `map` below, releasing the locked handle.
        //
        // SAFETY: `raw` is the valid, uniquely owned handle of the consumed decoder, and this is
        // its first probe (the `Decoder` type guarantees it was not probed before).
        check(unsafe { sys::uhdr_dec_probe(raw.as_ptr()) }).map(|()| probed)
    }
}

/// Probed UltraHDR decoder: the stream is parsed and the configuration is frozen.
///
/// Produced by [`Decoder::probe`]. The getters take `&self` and can be called while other views
/// are held. Decoding takes `&mut self`; [`reset`](Self::reset) turns the decoder back into a
/// configurable [`Decoder`].
///
/// Effects (crop, mirror, ...) may still be added until the first [`decode`](Self::decode):
/// libultrahdr locks them only when the codec runs.
#[derive(Debug)]
pub struct ProbedDecoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
}

// SAFETY: as for [`Decoder`]: a self-contained C++ handle with no thread-local state. Not `Sync`:
// the C API has no internal synchronization.
unsafe impl Send for ProbedDecoder {}

impl ProbedDecoder {
    /// All information reported by a probe: base image size, gain map size and metadata.
    pub fn info(&self) -> Result<ImageInfo> {
        let width = self.dimension(|raw| unsafe { sys::uhdr_dec_get_image_width(raw) })?;
        let height = self.dimension(|raw| unsafe { sys::uhdr_dec_get_image_height(raw) })?;
        let gainmap_width = unsafe { sys::uhdr_dec_get_gainmap_width(self.raw.as_ptr()) };
        let gainmap_height = unsafe { sys::uhdr_dec_get_gainmap_height(self.raw.as_ptr()) };
        let gainmap_size = (gainmap_width >= 0 && gainmap_height >= 0)
            .then_some((gainmap_width as u32, gainmap_height as u32));
        Ok(ImageInfo {
            width,
            height,
            gainmap_size,
            metadata: self.gainmap_metadata_direct(),
        })
    }

    /// Width of the base image in pixels.
    pub fn image_width(&self) -> Result<u32> {
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_image_width(raw) })
    }

    /// Height of the base image in pixels.
    pub fn image_height(&self) -> Result<u32> {
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_image_height(raw) })
    }

    /// Width of the gain map in pixels.
    pub fn gainmap_width(&self) -> Result<u32> {
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_gainmap_width(raw) })
    }

    /// Height of the gain map in pixels.
    pub fn gainmap_height(&self) -> Result<u32> {
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_gainmap_height(raw) })
    }

    /// Gain map metadata, or `None` if the stream carries none.
    pub fn gainmap_metadata(&self) -> Result<Option<GainMapMetadata>> {
        Ok(self.gainmap_metadata_direct())
    }

    /// EXIF payload of the base image, or `None` if the stream carries none.
    ///
    /// The bytes include the `"Exif\0\0"` identifier, so they can be fed straight back into
    /// [`Encoder::set_exif_data`](crate::Encoder::set_exif_data).
    pub fn exif(&self) -> Result<Option<MemBlockView<'_>>> {
        // SAFETY: the pointer, when non-null, is owned by the decoder until reset/drop.
        let ptr = unsafe { sys::uhdr_dec_get_exif(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// ICC profile of the base image, or `None` if the stream carries none.
    pub fn icc(&self) -> Result<Option<MemBlockView<'_>>> {
        // SAFETY: the pointer, when non-null, is owned by the decoder until reset/drop.
        let ptr = unsafe { sys::uhdr_dec_get_icc(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// Compressed (JPEG) base image, or `None` if unavailable.
    pub fn base_image(&self) -> Result<Option<MemBlockView<'_>>> {
        // SAFETY: the pointer, when non-null, is owned by the decoder until reset/drop.
        let ptr = unsafe { sys::uhdr_dec_get_base_image(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// Compressed (JPEG) gain map, or `None` if the stream has no gain map.
    pub fn gainmap_image(&self) -> Result<Option<MemBlockView<'_>>> {
        // SAFETY: the pointer, when non-null, is owned by the decoder until reset/drop.
        let ptr = unsafe { sys::uhdr_dec_get_gainmap_image(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// Decode the stream with the output profile configured before probing.
    ///
    /// A second call returns the buffers of the first decode: libultrahdr caches the decode
    /// result until the decoder is reset.
    pub fn decode(&mut self) -> Result<DecodedView<'_>> {
        // SAFETY: the handle is valid, owned by `self`, and carries an image (probing requires
        // one), so `uhdr_decode` can run.
        check(unsafe { sys::uhdr_decode(self.raw.as_ptr()) })?;
        // SAFETY: the buffer is owned by the decoder and outlives the returned borrow.
        let raw = unsafe { sys::uhdr_get_decoded_image(self.raw.as_ptr()) };
        let raw = unsafe { raw.as_mut() }
            .ok_or_else(|| Error::invalid_operation("decoder produced no image"))?;
        DecodedView::new(raw)
    }

    /// Decode the stream and borrow the base image and the gain map together.
    ///
    /// The two views in the returned [`DecodedFrame`] coexist, which separate calls to
    /// [`decode`](Self::decode) and [`decoded_gainmap`](Self::decoded_gainmap) do not allow.
    pub fn decode_with_gainmap(&mut self) -> Result<DecodedFrame<'_>> {
        // SAFETY: as in `decode`.
        check(unsafe { sys::uhdr_decode(self.raw.as_ptr()) })?;
        // SAFETY: both pointers, when non-null, are owned by the decoder and refer to distinct
        // internal buffers, so the two mutable borrows do not alias.
        let image = unsafe { sys::uhdr_get_decoded_image(self.raw.as_ptr()) };
        let image = unsafe { image.as_mut() }
            .ok_or_else(|| Error::invalid_operation("decoder produced no image"))?;
        let image = DecodedView::new(image)?;
        let gainmap = unsafe { sys::uhdr_get_decoded_gainmap_image(self.raw.as_ptr()) };
        let gainmap = match unsafe { gainmap.as_mut() } {
            Some(raw) => Some(DecodedView::new(raw)?),
            None => None,
        };
        Ok(DecodedFrame { image, gainmap })
    }

    /// Borrow the decoded gain map, if a gain map was produced.
    ///
    /// The format is [`PixelFormat::Mono8`](crate::PixelFormat::Mono8) for single-channel gain maps
    /// and [`PixelFormat::Rgba8888`](crate::PixelFormat::Rgba8888) for multi-channel ones. Requires
    /// a successful [`decode`](Self::decode); the view cannot be held at the same time as the one
    /// from `decode` — use [`decode_with_gainmap`](Self::decode_with_gainmap) for that.
    pub fn decoded_gainmap(&mut self) -> Result<Option<DecodedView<'_>>> {
        // SAFETY: the pointer, when non-null, is owned by the decoder until reset/drop.
        let ptr = unsafe { sys::uhdr_get_decoded_gainmap_image(self.raw.as_ptr()) };
        let Some(raw) = (unsafe { ptr.as_mut() }) else {
            return Ok(None);
        };
        Ok(Some(DecodedView::new(raw)?))
    }

    /// Enable/disable GPU acceleration. A no-op unless libultrahdr was built with GLES support.
    ///
    /// Must be called before the first [`decode`](Self::decode).
    pub fn enable_gpu_acceleration(&mut self, enable: bool) -> Result<()> {
        edit::enable_gpu_acceleration(self.raw, enable)
    }

    /// Mirror the decoded image. Effects apply in the order they are added, after decoding.
    ///
    /// Must be called before the first [`decode`](Self::decode).
    pub fn mirror(&mut self, direction: Mirror) -> Result<()> {
        edit::add_effect_mirror(self.raw, direction)
    }

    /// Rotate the decoded image clockwise.
    ///
    /// Must be called before the first [`decode`](Self::decode).
    pub fn rotate(&mut self, rotation: Rotation) -> Result<()> {
        edit::add_effect_rotate(self.raw, rotation)
    }

    /// Crop the decoded image to `rect`, given in absolute, exclusive pixel coordinates.
    ///
    /// Must be called before the first [`decode`](Self::decode).
    pub fn crop(&mut self, rect: CropRect) -> Result<()> {
        edit::add_effect_crop(self.raw, rect)
    }

    /// Resize the decoded image.
    ///
    /// Must be called before the first [`decode`](Self::decode).
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        edit::add_effect_resize(self.raw, width, height)
    }

    /// Reset all state and return a configurable [`Decoder`], which can be reused.
    pub fn reset(self) -> Decoder {
        // SAFETY: the handle is valid, owned by `self`, and not aliased (any view borrows `self`).
        unsafe { sys::uhdr_reset_decoder(self.raw.as_ptr()) };
        // Move the handle out of `self` (a `Drop` type) without running `ProbedDecoder::drop`,
        // which would release it.
        let raw = self.raw;
        std::mem::forget(self);
        Decoder {
            raw,
            image_set: false,
        }
    }

    /// Read one of the `uhdr_dec_get_*` dimension accessors, mapping `-1` to an error.
    fn dimension<F>(&self, get: F) -> Result<u32>
    where
        F: FnOnce(*mut sys::uhdr_codec_private_t) -> i32,
    {
        match get(self.raw.as_ptr()) {
            value if value < 0 => Err(Error::invalid_operation(
                "probe did not report the requested dimension",
            )),
            value => Ok(value as u32),
        }
    }

    /// Read the gain map metadata pointer (the stream is probed by construction).
    fn gainmap_metadata_direct(&self) -> Option<GainMapMetadata> {
        // SAFETY: the metadata, when present, is owned by the decoder and copied here.
        let ptr = unsafe { sys::uhdr_dec_get_gainmap_metadata(self.raw.as_ptr()) };
        let metadata = unsafe { ptr.as_ref() }?;
        Some(GainMapMetadata::from_sys(metadata))
    }

    /// Borrow one of the `uhdr_mem_block` accessors.
    fn mem_block(&self, ptr: *mut sys::uhdr_mem_block) -> Result<Option<MemBlockView<'_>>> {
        // SAFETY: the block, when present, is owned by the decoder until reset/drop; the returned
        // view borrows `&self`, which keeps the decoder (and the block) alive.
        let Some(block) = (unsafe { ptr.as_ref() }) else {
            return Ok(None);
        };
        if block.data_sz == 0 {
            return Ok(None);
        }
        Ok(Some(MemBlockView::new(
            block.data as *const u8,
            block.data_sz,
            block.capacity,
        )?))
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by `self` and released exactly once.
        unsafe { sys::uhdr_release_decoder(self.raw.as_ptr()) }
    }
}

impl Drop for ProbedDecoder {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by `self` and released exactly once.
        unsafe { sys::uhdr_release_decoder(self.raw.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_types_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Decoder>();
        assert_send::<ProbedDecoder>();
    }
}
