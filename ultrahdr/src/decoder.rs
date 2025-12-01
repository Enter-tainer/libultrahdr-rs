use crate::error::{check, Error, Result};
use crate::sys;
use crate::types::{ColorTransfer, CompressedImage, DecodedPackedView, GainMapMetadata, ImgFormat};
use std::ptr::NonNull;

pub struct Decoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
}

impl Decoder {
    pub fn new() -> Result<Self> {
        let ptr = unsafe { sys::uhdr_create_decoder() };
        NonNull::new(ptr)
            .map(|raw| Decoder { raw })
            .ok_or_else(Error::alloc)
    }

    pub fn set_image(&mut self, img: &mut CompressedImage<'_>) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_image(self.raw.as_ptr(), img.as_mut_ptr()) };
        check(err)
    }

    pub fn set_out_img_format(&mut self, fmt: ImgFormat) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_out_img_format(self.raw.as_ptr(), fmt) };
        check(err)
    }

    pub fn set_out_color_transfer(&mut self, ct: ColorTransfer) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_out_color_transfer(self.raw.as_ptr(), ct) };
        check(err)
    }

    pub fn set_out_max_display_boost(&mut self, boost: f32) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_set_out_max_display_boost(self.raw.as_ptr(), boost) };
        check(err)
    }

    pub fn probe(&mut self) -> Result<()> {
        let err = unsafe { sys::uhdr_dec_probe(self.raw.as_ptr()) };
        check(err)
    }

    /// Read gain map metadata (if present). Requires the image to be set.
    pub fn gainmap_metadata(&mut self) -> Result<Option<GainMapMetadata>> {
        self.probe()?;
        let ptr = unsafe { sys::uhdr_dec_get_gainmap_metadata(self.raw.as_ptr()) };
        if ptr.is_null() {
            return Ok(None);
        }
        // SAFETY: pointer owned by decoder; copied into owned struct.
        Ok(Some(GainMapMetadata::from_sys(unsafe { &*ptr })))
    }

    pub fn decode(&mut self) -> Result<()> {
        let err = unsafe { sys::uhdr_decode(self.raw.as_ptr()) };
        check(err)
    }

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
