use crate::error::{Error, Result, check};
use crate::sys;
use crate::types::{ColorTransfer, CompressedImage, DecodedPackedView, GainMapMetadata, ImgFormat};
use std::ptr::NonNull;

/// UltraHDR JPEG decoder. Owns the underlying `uhdr_codec_private_t` and provides
/// safe access to decoded pixel buffers and gain-map metadata.
pub struct Decoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
}

impl Decoder {
    /// Create a new decoder instance.
    pub fn new() -> Result<Self> {
        let ptr = unsafe { sys::uhdr_create_decoder() };
        NonNull::new(ptr)
            .map(|raw| Decoder { raw })
            .ok_or_else(Error::alloc)
    }

    /// Provide the compressed image to decode.
    pub fn set_image(&mut self, img: &mut CompressedImage<'_>) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_image(self.raw.as_ptr(), img.as_mut_ptr()) };
        check(err)
    }

    /// Choose the packed pixel layout for the decoded output.
    pub fn set_out_img_format(&mut self, fmt: ImgFormat) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_out_img_format(self.raw.as_ptr(), fmt) };
        check(err)
    }

    /// Choose the desired output transfer function (e.g. linear sRGB).
    pub fn set_out_color_transfer(&mut self, ct: ColorTransfer) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_out_color_transfer(self.raw.as_ptr(), ct) };
        check(err)
    }

    /// Clamp the maximum display boost applied by the decoder when reconstructing HDR.
    pub fn set_out_max_display_boost(&mut self, boost: f32) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_out_max_display_boost(self.raw.as_ptr(), boost) };
        check(err)
    }

    /// Parse the JPEG headers and any embedded gain map without decoding pixels.
    pub fn probe(&mut self) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_probe(self.raw.as_ptr()) };
        check(err)
    }

    /// Read gain map metadata (if present). Requires a previously set image.
    pub fn gainmap_metadata(&mut self) -> Result<Option<GainMapMetadata>> {
        self.probe()?;
        let ptr = unsafe { sys::uhdr_dec_get_gainmap_metadata(self.raw.as_ptr()) };
        if ptr.is_null() {
            return Ok(None);
        }
        // SAFETY: pointer owned by decoder; copied into owned struct.
        Ok(Some(GainMapMetadata::from_sys(unsafe { &*ptr })))
    }

    /// Decode the current image using the configured output format/transfer.
    pub fn decode(&mut self) -> Result<()> {
        let err = unsafe { sys::uhdr_decode(self.raw.as_ptr()) };
        check(err)
    }

    /// Decode into a packed pixel view with the provided format and transfer function.
    pub fn decode_packed_view(
        &mut self,
        fmt: ImgFormat,
        ct: ColorTransfer,
    ) -> Result<DecodedPackedView<'_>> {
        self.set_out_img_format(fmt)?;
        self.set_out_color_transfer(ct)?;
        self.decode()?;
        let raw = self
            .decoded_image()
            .ok_or_else(|| Error::invalid_param("decoded image is null"))?;
        DecodedPackedView::new(raw)
    }

    /// Borrow the decoded image owned by the decoder; remains valid until decoder is dropped/reset.
    pub(crate) fn decoded_image(&mut self) -> Option<&mut sys::uhdr_raw_image> {
        let ptr = unsafe { sys::uhdr_get_decoded_image(self.raw.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *ptr })
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { sys::uhdr_release_decoder(self.raw.as_ptr()) }
    }
}
