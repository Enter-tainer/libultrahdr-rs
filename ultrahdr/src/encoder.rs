//! UltraHDR encoder.
//!
//! ```no_run
//! use ultrahdr::{ColorAspects, ColorGamut, ColorRange, ColorTransfer, Codec, Encoder, ImageLabel,
//!                PixelFormat, RawImage};
//!
//! # fn main() -> ultrahdr::Result<()> {
//! let aspects = ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);
//! let hdr = RawImage::new(PixelFormat::Rgba1010102, 640, 480, aspects)?;
//!
//! let mut enc = Encoder::new()?;
//! enc.set_raw_image(ImageLabel::Hdr, &hdr)?;
//! enc.set_quality(ImageLabel::Hdr, 95)?;
//! enc.set_output_format(Codec::Jpeg)?;
//! enc.encode()?;
//!
//! let stream = enc.encoded_stream().expect("encoder produced no output");
//! std::fs::write("out.jpg", stream.bytes()).expect("write stream");
//! # Ok(())
//! # }
//! ```

use crate::GainMapMetadata;
use crate::edit;
use crate::edit::{Mirror, Rotation};
use crate::error::{Error, Result, check};
use crate::image::{
    Codec, CompressedImage, DecodedView, EncodedView, ImageLabel, Preset, RawImage,
};
use crate::sys;
use std::ffi::c_void;
use std::ptr::NonNull;

/// UltraHDR encoder. Owns the underlying `uhdr_codec_private_t` and can be reused by calling
/// [`reset`](Self::reset).
#[derive(Debug)]
pub struct Encoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
}

impl Encoder {
    /// Create a new encoder instance.
    pub fn new() -> Result<Self> {
        // SAFETY: create returns an owned handle or null.
        let raw =
            NonNull::new(unsafe { sys::uhdr_create_encoder() }).ok_or_else(Error::allocation)?;
        Ok(Self { raw })
    }

    /// Register an uncompressed image as the `label` intent.
    ///
    /// libultrahdr copies the pixels, so `image` may be dropped as soon as this returns.
    pub fn set_raw_image(&mut self, label: ImageLabel, image: &RawImage) -> Result<()> {
        let mut raw = image.as_sys();
        // SAFETY: the library copies the described planes during the call.
        check(unsafe { sys::uhdr_enc_set_raw_image(self.raw.as_ptr(), &mut raw, label.to_sys()) })
    }

    /// Register pixels decoded by a [`Decoder`](crate::Decoder) without copying them first.
    ///
    /// This is the zero-copy counterpart of [`set_raw_image`](Self::set_raw_image): the view
    /// borrows the decoder's buffer, which libultrahdr then copies during the call.
    pub fn set_decoded_image(&mut self, label: ImageLabel, image: &DecodedView<'_>) -> Result<()> {
        let mut raw = image.as_sys();
        // SAFETY: the library copies the described planes during the call.
        check(unsafe { sys::uhdr_enc_set_raw_image(self.raw.as_ptr(), &mut raw, label.to_sys()) })
    }

    /// Register a pre-compressed image (JPEG) as the `label` intent.
    ///
    /// With [`ImageLabel::Hdr`]/[`ImageLabel::Sdr`] the compressed image is decoded and a gain map
    /// is computed from it. With [`ImageLabel::Base`]/[`ImageLabel::GainMap`] the stream is taken
    /// as-is (or transcoded) and combined with the metadata passed to
    /// [`set_gainmap_image`](Self::set_gainmap_image).
    pub fn set_compressed_image(
        &mut self,
        label: ImageLabel,
        image: &CompressedImage,
    ) -> Result<()> {
        let mut raw = image.as_sys();
        // SAFETY: the library copies the stream during the call.
        check(unsafe {
            sys::uhdr_enc_set_compressed_image(self.raw.as_ptr(), &mut raw, label.to_sys())
        })
    }

    /// Supply a pre-computed gain map image together with the metadata that describes it,
    /// bypassing gain map computation.
    ///
    /// Requires the base image to have been registered as a compressed image with
    /// [`ImageLabel::Base`]. The metadata is used verbatim: settings such as
    /// [`set_gainmap_gamma`](Self::set_gainmap_gamma) do not affect it.
    pub fn set_gainmap_image(
        &mut self,
        image: &CompressedImage,
        metadata: &GainMapMetadata,
    ) -> Result<()> {
        let mut raw = image.as_sys();
        let mut metadata = sys::uhdr_gainmap_metadata::from(metadata);
        // SAFETY: both descriptors are read (and copied) during the call.
        check(unsafe {
            sys::uhdr_enc_set_gainmap_image(self.raw.as_ptr(), &mut raw, &mut metadata)
        })
    }

    /// Attach EXIF data to the encoded stream.
    ///
    /// The payload is copied verbatim into an `APP1` segment, so it must already start with the
    /// `"Exif\0\0"` identifier — exactly the bytes [`Decoder::exif`](crate::Decoder::exif) returns
    /// when reading a stream back. The library neither generates nor validates EXIF itself.
    pub fn set_exif_data(&mut self, exif: impl AsRef<[u8]>) -> Result<()> {
        let exif = exif.as_ref();
        if exif.is_empty() {
            return Err(Error::invalid_parameter("EXIF payload is empty"));
        }
        let mut block = sys::uhdr_mem_block_t {
            data: exif.as_ptr() as *mut c_void,
            data_sz: exif.len(),
            capacity: exif.len(),
        };
        // SAFETY: the library copies the payload during the call.
        check(unsafe { sys::uhdr_enc_set_exif_data(self.raw.as_ptr(), &mut block) })
    }

    /// Set the JPEG quality (`0..=100`) for base and gain map images.
    pub fn set_quality(&mut self, label: ImageLabel, quality: u8) -> Result<()> {
        if quality > 100 {
            return Err(Error::invalid_parameter(format!(
                "quality {quality} is out of range 0..=100"
            )));
        }
        // SAFETY: the handle is valid and owned by `self`.
        check(unsafe {
            sys::uhdr_enc_set_quality(self.raw.as_ptr(), i32::from(quality), label.to_sys())
        })
    }

    /// Recommend the minimum and maximum content boost, in linear scale.
    ///
    /// libultrahdr treats these as recommendations and may pick different values; the resulting
    /// values are reported by [`Decoder::gainmap_metadata`](crate::Decoder::gainmap_metadata).
    pub fn set_min_max_content_boost(&mut self, min_boost: f32, max_boost: f32) -> Result<()> {
        check(unsafe {
            sys::uhdr_enc_set_min_max_content_boost(self.raw.as_ptr(), min_boost, max_boost)
        })
    }

    /// Set the gain map scale factor (larger values bias towards HDR detail).
    pub fn set_gainmap_scale_factor(&mut self, factor: i32) -> Result<()> {
        check(unsafe { sys::uhdr_enc_set_gainmap_scale_factor(self.raw.as_ptr(), factor) })
    }

    /// Enable or disable multi-channel gain maps.
    pub fn set_multi_channel_gainmap(&mut self, enable: bool) -> Result<()> {
        check(unsafe {
            sys::uhdr_enc_set_using_multi_channel_gainmap(self.raw.as_ptr(), i32::from(enable))
        })
    }

    /// Adjust the gain map gamma curve.
    pub fn set_gainmap_gamma(&mut self, gamma: f32) -> Result<()> {
        check(unsafe { sys::uhdr_enc_set_gainmap_gamma(self.raw.as_ptr(), gamma) })
    }

    /// Set the target display peak brightness in nits, used for capacity calculations.
    pub fn set_target_display_peak_brightness(&mut self, nits: f32) -> Result<()> {
        check(unsafe { sys::uhdr_enc_set_target_display_peak_brightness(self.raw.as_ptr(), nits) })
    }

    /// Choose a tuning preset.
    pub fn set_preset(&mut self, preset: Preset) -> Result<()> {
        check(unsafe { sys::uhdr_enc_set_preset(self.raw.as_ptr(), preset.to_sys()) })
    }

    /// Choose the container produced by [`encode`](Self::encode).
    pub fn set_output_format(&mut self, codec: Codec) -> Result<()> {
        check(unsafe { sys::uhdr_enc_set_output_format(self.raw.as_ptr(), codec.to_sys()) })
    }

    /// Run the encoder with the current configuration.
    pub fn encode(&mut self) -> Result<()> {
        check(unsafe { sys::uhdr_encode(self.raw.as_ptr()) })
    }

    /// Borrow the stream produced by [`encode`](Self::encode), if any.
    pub fn encoded_stream(&mut self) -> Option<EncodedView<'_>> {
        // SAFETY: the pointer is owned by the encoder and valid until the next encode/reset/drop.
        let ptr = unsafe { sys::uhdr_get_encoded_stream(self.raw.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(EncodedView::new(unsafe { &mut *ptr }))
        }
    }

    /// Enable/disable GPU acceleration. A no-op unless libultrahdr was built with GLES support.
    pub fn enable_gpu_acceleration(&mut self, enable: bool) -> Result<()> {
        edit::enable_gpu_acceleration(self.raw, enable)
    }

    /// Mirror the input images before encoding. Effects apply in the order they are added.
    ///
    /// Only supported for raw image inputs, not for compressed intents.
    pub fn mirror(&mut self, direction: Mirror) -> Result<()> {
        edit::add_effect_mirror(self.raw, direction)
    }

    /// Rotate the input images clockwise before encoding.
    pub fn rotate(&mut self, rotation: Rotation) -> Result<()> {
        edit::add_effect_rotate(self.raw, rotation)
    }

    /// Crop the input images before encoding, given absolute, exclusive pixel coordinates.
    ///
    /// The output is `(right - left)` × `(bottom - top)` pixels.
    pub fn crop(&mut self, left: i32, right: i32, top: i32, bottom: i32) -> Result<()> {
        edit::add_effect_crop(self.raw, left, right, top, bottom)
    }

    /// Resize the input images before encoding.
    pub fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        edit::add_effect_resize(self.raw, width, height)
    }

    /// Reset all state so the encoder can be reused.
    pub fn reset(&mut self) {
        // SAFETY: the handle is valid and owned by `self`.
        unsafe { sys::uhdr_reset_encoder(self.raw.as_ptr()) }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by `self` and released exactly once.
        unsafe { sys::uhdr_release_encoder(self.raw.as_ptr()) }
    }
}
