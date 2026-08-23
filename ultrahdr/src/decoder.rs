use crate::error::{Error, Result, check};
use crate::sys;
use crate::types::{
    ColorTransfer, CompressedImage, DecodedPackedView, GainMapMetadata, ImgFormat, ParsedUltraHdr,
    UltraHdrContainer,
};
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

    /// Compressed JPEG bytes of the primary (SDR base) image, exposed by the
    /// decoder without decoding pixels. Requires `probe()` first.
    pub fn base_image(&mut self) -> Option<&[u8]> {
        unsafe { mem_block_slice(sys::uhdr_dec_get_base_image(self.raw.as_ptr())) }
    }

    /// Compressed JPEG bytes of the embedded gain map, exposed by the decoder
    /// without decoding pixels. Requires `probe()` first.
    pub fn gainmap_image(&mut self) -> Option<&[u8]> {
        unsafe { mem_block_slice(sys::uhdr_dec_get_gainmap_image(self.raw.as_ptr())) }
    }

    /// Raw EXIF block, if present. Requires `probe()` first.
    pub fn exif(&mut self) -> Option<&[u8]> {
        unsafe { mem_block_slice(sys::uhdr_dec_get_exif(self.raw.as_ptr())) }
    }

    /// Raw ICC profile block, if present. Requires `probe()` first.
    pub fn icc(&mut self) -> Option<&[u8]> {
        unsafe { mem_block_slice(sys::uhdr_dec_get_icc(self.raw.as_ptr())) }
    }

    /// Primary image width in pixels. Requires `probe()` first.
    pub fn image_width(&mut self) -> usize {
        unsafe { sys::uhdr_dec_get_image_width(self.raw.as_ptr()) as usize }
    }

    /// Primary image height in pixels. Requires `probe()` first.
    pub fn image_height(&mut self) -> usize {
        unsafe { sys::uhdr_dec_get_image_height(self.raw.as_ptr()) as usize }
    }

    /// Gain map width in pixels. Requires `probe()` first.
    pub fn gainmap_width(&mut self) -> usize {
        unsafe { sys::uhdr_dec_get_gainmap_width(self.raw.as_ptr()) as usize }
    }

    /// Gain map height in pixels. Requires `probe()` first.
    pub fn gainmap_height(&mut self) -> usize {
        unsafe { sys::uhdr_dec_get_gainmap_height(self.raw.as_ptr()) as usize }
    }

    /// Parse the UltraHDR container and return everything needed to render it in
    /// a browser, without decoding pixels.
    ///
    /// This is the ergonomic entry point for a JS/WebGPU renderer: it probes the
    /// container, copies the base and gain-map JPEG bytes into owned buffers (so
    /// the result crosses the wasm/JS boundary freely), and exposes the gain-map
    /// parameters. See [`ParsedUltraHdr`].
    pub fn parse_layout(&mut self) -> Result<ParsedUltraHdr> {
        self.probe()?;
        let gainmap_image = self.gainmap_image().map(|s| s.to_vec()).unwrap_or_default();
        let gainmap_metadata = self.gainmap_metadata()?;
        let exif = self.exif().map(|s| s.to_vec());
        let icc = self.icc().map(|s| s.to_vec());
        Ok(ParsedUltraHdr {
            // The Decoder doesn't hold the input bytes, so the container is not
            // known here. Use the top-level [`parse_ultra_hdr`] to get it detected.
            container: UltraHdrContainer::Unknown,
            gainmap_image,
            gainmap_metadata,
            width: self.image_width() as u32,
            height: self.image_height() as u32,
            gainmap_width: self.gainmap_width() as u32,
            gainmap_height: self.gainmap_height() as u32,
            exif,
            icc,
        })
    }
}

/// Interpret a `uhdr_mem_block_t*` returned by a decoder accessor as a `&[u8]`.
/// The block is owned by the decoder, so the slice borrows from it.
unsafe fn mem_block_slice<'a>(ptr: *mut sys::uhdr_mem_block_t) -> Option<&'a [u8]> {
    if ptr.is_null() {
        return None;
    }
    let block = unsafe { &*ptr };
    if block.data.is_null() || block.data_sz == 0 {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(block.data as *const u8, block.data_sz) })
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { sys::uhdr_release_decoder(self.raw.as_ptr()) }
    }
}

/// Parse an UltraHDR image (JPEG, HEIF or AVIF container) and return everything
/// needed to render it in a browser, without decoding pixels.
///
/// This is the ergonomic one-call entry point: it detects the container from the
/// bytes, probes it, and returns the owned base/gain-map image bytes plus the
/// gain-map metadata. Enable the `heif` feature to support HEIF/AVIF input.
pub fn parse_ultra_hdr(bytes: &mut [u8]) -> Result<ParsedUltraHdr> {
    let container = detect_container(bytes);
    let mut comp = CompressedImage::from_bytes(
        bytes,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    let mut dec = Decoder::new()?;
    dec.set_image(&mut comp)?;
    let mut layout = dec.parse_layout()?;
    layout.container = container;
    Ok(layout)
}

/// Detect whether `bytes` is a JPEG, HEIF or AVIF container.
///
/// JPEG begins with a SOI marker (`FF D8`); HEIF and AVIF both use an ISO BMFF
/// `ftyp` box at offset 4, distinguished by the brand at offset 8.
fn detect_container(bytes: &[u8]) -> UltraHdrContainer {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        return UltraHdrContainer::Jpeg;
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        match &bytes[8..12] {
            b"avif" | b"avis" => return UltraHdrContainer::Avif,
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => {
                return UltraHdrContainer::Heif;
            }
            _ => {}
        }
    }
    UltraHdrContainer::Unknown
}
