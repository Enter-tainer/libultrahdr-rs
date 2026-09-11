//! UltraHDR decoder.
//!
//! # State machine
//!
//! libultrahdr freezes a decoder as soon as it is probed: neither the output configuration nor the
//! input image can change until [`reset`](Decoder::reset). This wrapper tracks that state so the
//! information getters and [`decode`](Decoder::decode) cooperate instead of failing with an opaque
//! `UHDR_CODEC_INVALID_OPERATION`. Configure the output *before* reading information:
//!
//! ```no_run
//! use ultrahdr::{ColorTransfer, CompressedImage, Decoder, PixelFormat};
//!
//! # fn main() -> ultrahdr::Result<()> {
//! # let bytes: Vec<u8> = Vec::new();
//! let mut dec = Decoder::new()?;
//! dec.set_image(&CompressedImage::new(bytes))?;
//! // Settings first, then probing (implicit or explicit).
//! dec.set_output_format(PixelFormat::Rgba1010102)?;
//! dec.set_output_transfer(ColorTransfer::Pq)?;
//!
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
use crate::edit::{Mirror, Rotation};
use crate::error::{Error, Result, check};
use crate::image::{ColorTransfer, CompressedImage, DecodedView, MemBlockView, PixelFormat};
use crate::sys;
use std::ffi::c_void;
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
    unsafe { sys::is_uhdr_image(data.as_ptr() as *mut c_void, data.len() as i32) != 0 }
}

/// Output configuration a fresh (or reset) decoder starts with; mirrors `uhdr_reset_decoder`.
const DEFAULT_OUT_FORMAT: PixelFormat = PixelFormat::RgbaHalfFloat;
const DEFAULT_OUT_TRANSFER: ColorTransfer = ColorTransfer::Linear;

/// Information about a stream, as reported by [`Decoder::probe`].
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

/// UltraHDR decoder. Owns the underlying `uhdr_codec_private_t` and provides safe access to the
/// decoded pixels, the gain map and the embedded metadata.
#[derive(Debug)]
pub struct Decoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
    /// An image has been registered via [`Decoder::set_image`].
    image_set: bool,
    /// Mirrors the C context's one-shot probe lock.
    probed: bool,
    /// Output format most recently accepted by the C context.
    out_format: PixelFormat,
    /// Output transfer function most recently accepted by the C context.
    out_transfer: ColorTransfer,
}

impl Decoder {
    /// Create a new decoder instance.
    pub fn new() -> Result<Self> {
        // SAFETY: create returns an owned handle or null.
        let raw =
            NonNull::new(unsafe { sys::uhdr_create_decoder() }).ok_or_else(Error::allocation)?;
        Ok(Self {
            raw,
            image_set: false,
            probed: false,
            out_format: DEFAULT_OUT_FORMAT,
            out_transfer: DEFAULT_OUT_TRANSFER,
        })
    }

    /// Check whether `data` is an UltraHDR stream. See the free function [`is_uhdr_image`].
    pub fn is_uhdr_image(data: &[u8]) -> bool {
        is_uhdr_image(data)
    }

    /// Register the compressed image to decode.
    ///
    /// libultrahdr copies the stream, so `image` may be dropped as soon as this returns.
    pub fn set_image(&mut self, image: &CompressedImage) -> Result<()> {
        self.ensure_configurable("set a new image")?;
        let mut raw = image.as_sys();
        // SAFETY: the library copies the stream during the call.
        check(unsafe { sys::uhdr_dec_set_image(self.raw.as_ptr(), &mut raw) })?;
        self.image_set = true;
        Ok(())
    }

    /// Choose the packed pixel layout of the decoded output.
    ///
    /// Must be called before the decoder is probed; see the type-level documentation.
    pub fn set_output_format(&mut self, format: PixelFormat) -> Result<()> {
        self.ensure_configurable("change the output pixel format")?;
        // SAFETY: the handle is valid and owned by `self`.
        check(unsafe { sys::uhdr_dec_set_out_img_format(self.raw.as_ptr(), format.to_sys()) })?;
        self.out_format = format;
        Ok(())
    }

    /// Choose the transfer function of the decoded output.
    ///
    /// Must be called before the decoder is probed; see the type-level documentation.
    pub fn set_output_transfer(&mut self, transfer: ColorTransfer) -> Result<()> {
        self.ensure_configurable("change the output transfer function")?;
        // SAFETY: the handle is valid and owned by `self`.
        check(unsafe {
            sys::uhdr_dec_set_out_color_transfer(self.raw.as_ptr(), transfer.to_sys())
        })?;
        self.out_transfer = transfer;
        Ok(())
    }

    /// Clamp the maximum display boost applied when reconstructing HDR.
    ///
    /// Must be called before the decoder is probed; see the type-level documentation.
    pub fn set_max_display_boost(&mut self, boost: f32) -> Result<()> {
        self.ensure_configurable("change the maximum display boost")?;
        check(unsafe { sys::uhdr_dec_set_out_max_display_boost(self.raw.as_ptr(), boost) })
    }

    /// Parse the stream headers, gain map metadata and embedded blocks without decoding pixels.
    ///
    /// Probing freezes the configuration, so call the `set_output_*` methods first. The information
    /// getters on this type probe on demand.
    pub fn probe(&mut self) -> Result<()> {
        if !self.image_set {
            return Err(Error::invalid_operation(
                "decoder has no image; call set_image before probing",
            ));
        }
        // The C context locks itself as soon as probe runs, even if parsing then fails.
        self.probed = true;
        check(unsafe { sys::uhdr_dec_probe(self.raw.as_ptr()) })
    }

    /// All information reported by a probe: base image size, gain map size and metadata.
    pub fn info(&mut self) -> Result<ImageInfo> {
        self.probe()?;
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

    /// Width of the base image in pixels. Probes the stream first if needed.
    pub fn image_width(&mut self) -> Result<u32> {
        self.probe()?;
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_image_width(raw) })
    }

    /// Height of the base image in pixels. Probes the stream first if needed.
    pub fn image_height(&mut self) -> Result<u32> {
        self.probe()?;
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_image_height(raw) })
    }

    /// Width of the gain map in pixels. Probes the stream first if needed.
    pub fn gainmap_width(&mut self) -> Result<u32> {
        self.probe()?;
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_gainmap_width(raw) })
    }

    /// Height of the gain map in pixels. Probes the stream first if needed.
    pub fn gainmap_height(&mut self) -> Result<u32> {
        self.probe()?;
        self.dimension(|raw| unsafe { sys::uhdr_dec_get_gainmap_height(raw) })
    }

    /// Gain map metadata, or `None` if the stream carries none. Probes the stream first if needed.
    pub fn gainmap_metadata(&mut self) -> Result<Option<GainMapMetadata>> {
        self.probe()?;
        Ok(self.gainmap_metadata_direct())
    }

    /// EXIF payload of the base image, or `None` if the stream carries none.
    ///
    /// The bytes include the `"Exif\0\0"` identifier, so they can be fed straight back into
    /// [`Encoder::set_exif_data`](crate::Encoder::set_exif_data).
    pub fn exif(&mut self) -> Result<Option<MemBlockView<'_>>> {
        self.probe()?;
        let ptr = unsafe { sys::uhdr_dec_get_exif(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// ICC profile of the base image, or `None` if the stream carries none.
    pub fn icc(&mut self) -> Result<Option<MemBlockView<'_>>> {
        self.probe()?;
        let ptr = unsafe { sys::uhdr_dec_get_icc(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// Compressed (JPEG) base image, or `None` if unavailable.
    pub fn base_image(&mut self) -> Result<Option<MemBlockView<'_>>> {
        self.probe()?;
        let ptr = unsafe { sys::uhdr_dec_get_base_image(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// Compressed (JPEG) gain map, or `None` if the stream has no gain map.
    pub fn gainmap_image(&mut self) -> Result<Option<MemBlockView<'_>>> {
        self.probe()?;
        let ptr = unsafe { sys::uhdr_dec_get_gainmap_image(self.raw.as_ptr()) };
        self.mem_block(ptr)
    }

    /// Decode the stream with the currently configured output settings.
    ///
    /// See the type-level documentation for the configuration order.
    pub fn decode(&mut self) -> Result<DecodedView<'_>> {
        if !self.image_set {
            return Err(Error::invalid_operation(
                "decoder has no image; call set_image before decoding",
            ));
        }
        // `uhdr_decode` probes internally, which locks the context.
        self.probed = true;
        check(unsafe { sys::uhdr_decode(self.raw.as_ptr()) })?;
        // SAFETY: the buffer is owned by the decoder and outlives the returned borrow.
        let raw = unsafe { sys::uhdr_get_decoded_image(self.raw.as_ptr()) };
        let raw = unsafe { raw.as_mut() }
            .ok_or_else(|| Error::invalid_operation("decoder produced no image"))?;
        DecodedView::new(raw)
    }

    /// Configure the output and decode in one call.
    ///
    /// Equivalent to [`set_output_format`](Self::set_output_format) +
    /// [`set_output_transfer`](Self::set_output_transfer) + [`decode`](Self::decode). Changing the
    /// output after the decoder has been probed is impossible in the C API, so this returns an
    /// error unless the requested settings are already in effect.
    pub fn decode_as(
        &mut self,
        format: PixelFormat,
        transfer: ColorTransfer,
    ) -> Result<DecodedView<'_>> {
        if self.out_format != format || self.out_transfer != transfer {
            self.ensure_configurable("change the output format for decoding")?;
            self.set_output_format(format)?;
            self.set_output_transfer(transfer)?;
        }
        self.decode()
    }

    /// Borrow the decoded gain map, if a gain map was produced.
    ///
    /// The format is [`PixelFormat::Mono8`] for single-channel gain maps and
    /// [`PixelFormat::Rgba8888`] for multi-channel ones. Requires a successful
    /// [`decode`](Self::decode).
    pub fn decoded_gainmap(&mut self) -> Result<Option<DecodedView<'_>>> {
        // SAFETY: the pointer, when non-null, is owned by the decoder until reset/drop.
        let ptr = unsafe { sys::uhdr_get_decoded_gainmap_image(self.raw.as_ptr()) };
        let Some(raw) = (unsafe { ptr.as_mut() }) else {
            return Ok(None);
        };
        Ok(Some(DecodedView::new(raw)?))
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

    /// Crop the decoded image, given absolute, exclusive pixel coordinates.
    ///
    /// The output is `(right - left)` × `(bottom - top)` pixels.
    pub fn crop(&mut self, left: i32, right: i32, top: i32, bottom: i32) -> Result<()> {
        edit::add_effect_crop(self.raw, left, right, top, bottom)
    }

    /// Resize the decoded image.
    pub fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        edit::add_effect_resize(self.raw, width, height)
    }

    /// Reset all state so the decoder can be reused.
    pub fn reset(&mut self) {
        // SAFETY: the handle is valid and owned by `self`.
        unsafe { sys::uhdr_reset_decoder(self.raw.as_ptr()) }
        self.image_set = false;
        self.probed = false;
        self.out_format = DEFAULT_OUT_FORMAT;
        self.out_transfer = DEFAULT_OUT_TRANSFER;
    }

    /// Reject configuration changes the C context would refuse because it was probed.
    fn ensure_configurable(&self, action: &str) -> Result<()> {
        if self.probed {
            return Err(Error::invalid_operation(format!(
                "cannot {action}: the decoder was already probed and its configuration is frozen; \
                 configure the decoder before probing, or call reset()"
            )));
        }
        Ok(())
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

    /// Read the gain map metadata pointer without probing (the caller must have probed).
    fn gainmap_metadata_direct(&self) -> Option<GainMapMetadata> {
        // SAFETY: the metadata, when present, is owned by the decoder and copied here.
        let ptr = unsafe { sys::uhdr_dec_get_gainmap_metadata(self.raw.as_ptr()) };
        let metadata = unsafe { ptr.as_ref() }?;
        Some(GainMapMetadata::from_sys(metadata))
    }

    /// Borrow one of the `uhdr_mem_block` accessors.
    fn mem_block(&self, ptr: *mut sys::uhdr_mem_block) -> Result<Option<MemBlockView<'_>>> {
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
