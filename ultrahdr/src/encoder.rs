use crate::error::{Error, Result, check};
use crate::sys;
use crate::types::{
    Codec, CompressedImage, DecodedPackedView, EncPreset, EncodedView, ImgLabel, OwnedPackedImage,
    RawImage,
};
use std::ptr::NonNull;

pub struct Encoder {
    raw: NonNull<sys::uhdr_codec_private_t>,
}

impl Encoder {
    pub fn new() -> Result<Self> {
        let ptr = unsafe { sys::uhdr_create_encoder() };
        NonNull::new(ptr)
            .map(|raw| Encoder { raw })
            .ok_or_else(Error::alloc)
    }

    pub fn set_raw_image(&mut self, img: &mut RawImage<'_>, intent: ImgLabel) -> Result<()> {
        let err =
            unsafe { sys::uhdr_enc_set_raw_image(self.raw.as_ptr(), img.as_mut_ptr(), intent) };
        check(err)
    }

    pub fn set_raw_image_view(
        &mut self,
        img: &mut DecodedPackedView<'_>,
        intent: ImgLabel,
    ) -> Result<()> {
        let err =
            unsafe { sys::uhdr_enc_set_raw_image(self.raw.as_ptr(), img.as_raw_mut(), intent) };
        check(err)
    }

    pub fn set_raw_owned_image(
        &mut self,
        img: &mut OwnedPackedImage,
        intent: ImgLabel,
    ) -> Result<()> {
        let err =
            unsafe { sys::uhdr_enc_set_raw_image(self.raw.as_ptr(), img.as_raw_mut(), intent) };
        check(err)
    }

    pub fn set_compressed_image(
        &mut self,
        img: &mut CompressedImage<'_>,
        intent: ImgLabel,
    ) -> Result<()> {
        let err = unsafe {
            sys::uhdr_enc_set_compressed_image(self.raw.as_ptr(), img.as_mut_ptr(), intent)
        };
        check(err)
    }

    pub fn set_quality(&mut self, quality: i32, label: ImgLabel) -> Result<()> {
        let err = unsafe { sys::uhdr_enc_set_quality(self.raw.as_ptr(), quality, label) };
        check(err)
    }

    pub fn set_gainmap_scale_factor(&mut self, factor: i32) -> Result<()> {
        let err = unsafe { sys::uhdr_enc_set_gainmap_scale_factor(self.raw.as_ptr(), factor) };
        check(err)
    }

    pub fn set_using_multi_channel_gainmap(&mut self, enable: bool) -> Result<()> {
        let err = unsafe {
            sys::uhdr_enc_set_using_multi_channel_gainmap(self.raw.as_ptr(), enable as i32)
        };
        check(err)
    }

    pub fn set_gainmap_gamma(&mut self, gamma: f32) -> Result<()> {
        let err = unsafe { sys::uhdr_enc_set_gainmap_gamma(self.raw.as_ptr(), gamma) };
        check(err)
    }

    pub fn set_target_display_peak_brightness(&mut self, nits: f32) -> Result<()> {
        let err =
            unsafe { sys::uhdr_enc_set_target_display_peak_brightness(self.raw.as_ptr(), nits) };
        check(err)
    }

    pub fn set_preset(&mut self, preset: EncPreset) -> Result<()> {
        let err = unsafe { sys::uhdr_enc_set_preset(self.raw.as_ptr(), preset) };
        check(err)
    }

    pub fn set_output_format(&mut self, codec: Codec) -> Result<()> {
        let err = unsafe { sys::uhdr_enc_set_output_format(self.raw.as_ptr(), codec) };
        check(err)
    }

    pub fn encode(&mut self) -> Result<()> {
        let err = unsafe { sys::uhdr_encode(self.raw.as_ptr()) };
        check(err)
    }

    /// Returns a view of the encoded stream owned by the encoder.
    pub fn encoded_stream(&mut self) -> Option<EncodedView<'_>> {
        let ptr = unsafe { sys::uhdr_get_encoded_stream(self.raw.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(EncodedView::new(unsafe { &*ptr }))
        }
    }

    pub fn reset(&mut self) {
        unsafe { sys::uhdr_reset_encoder(self.raw.as_ptr()) }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { sys::uhdr_release_encoder(self.raw.as_ptr()) }
    }
}
