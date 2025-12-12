use crate::error::{Error, Result};
use crate::sys;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr;

/// Pixel layout for packed buffers accepted/returned by libultrahdr.
pub type ImgFormat = sys::uhdr_img_fmt_t;
/// Scene-referred color gamut.
pub type ColorGamut = sys::uhdr_color_gamut_t;
/// Transfer function describing the relationship between encoded values and scene light.
pub type ColorTransfer = sys::uhdr_color_transfer_t;
/// Chroma sample range.
pub type ColorRange = sys::uhdr_color_range_t;
/// Output codec to write.
pub type Codec = sys::uhdr_codec_t;
/// Whether an image is the base view or the gain map.
pub type ImgLabel = sys::uhdr_img_label_t;
/// Encoder tuning preset.
pub type EncPreset = sys::uhdr_enc_preset_t;
/// Error codes returned by the underlying C API.
pub type ErrorCode = sys::uhdr_codec_err_t;

/// Nominal SDR diffuse white used by libultrahdr for capacity math (ISO/TS 22028-5).
pub const SDR_WHITE_NITS: f32 = 203.0;

/// Owned compressed JPEG (and optional gain-map) returned by an [`Encoder`].
#[derive(Debug, Clone)]
pub struct EncodedImage {
    /// Encoded byte stream.
    pub data: Vec<u8>,
    /// Color gamut of the encoded stream.
    pub cg: ColorGamut,
    /// Transfer function of the encoded stream.
    pub ct: ColorTransfer,
    /// Chroma range of the encoded stream.
    pub range: ColorRange,
}

/// Borrowed view over an encoded stream owned by an [`Encoder`] or [`Decoder`].
#[derive(Debug, Copy, Clone)]
pub struct EncodedView<'a> {
    inner: &'a sys::uhdr_compressed_image,
}

impl<'a> EncodedView<'a> {
    pub(crate) fn new(inner: &'a sys::uhdr_compressed_image) -> Self {
        Self { inner }
    }

    /// Return the JPEG/gain-map bytes.
    pub fn bytes(&self) -> Result<&'a [u8]> {
        if self.inner.data.is_null() {
            return Err(Error::invalid_param("null compressed data"));
        }
        if self.inner.data_sz > self.inner.capacity {
            return Err(Error::invalid_param("compressed size exceeds capacity"));
        }
        // SAFETY: bounded by data_sz verified above.
        let slice =
            unsafe { std::slice::from_raw_parts(self.inner.data as *const u8, self.inner.data_sz) };
        Ok(slice)
    }

    /// Color metadata attached to the encoded stream.
    pub fn meta(&self) -> (ColorGamut, ColorTransfer, ColorRange) {
        (self.inner.cg, self.inner.ct, self.inner.range)
    }

    /// Copy the stream into an owned buffer.
    pub fn to_owned(&self) -> Result<EncodedImage> {
        let data = copy_compressed_image(self.inner)?;
        let (cg, ct, range) = self.meta();
        Ok(EncodedImage {
            data,
            cg,
            ct,
            range,
        })
    }
}

/// Owned packed pixels plus metadata returned by a [`Decoder`].
#[derive(Debug, Clone)]
pub struct DecodedPacked {
    /// Output pixel layout.
    pub fmt: ImgFormat,
    /// Output color gamut.
    pub cg: ColorGamut,
    /// Output transfer function.
    pub ct: ColorTransfer,
    /// Output chroma range.
    pub range: ColorRange,
    /// Logical width in pixels.
    pub width: u32,
    /// Logical height in pixels.
    pub height: u32,
    /// Pixel buffer tightly packed to width.
    pub data: Vec<u8>,
}

impl DecodedPacked {
    /// Re-expose the owned pixels as a [`RawImage`] descriptor.
    pub fn as_raw_image(&mut self) -> Result<RawImage<'_>> {
        RawImage::packed(
            self.fmt,
            self.width,
            self.height,
            &mut self.data,
            self.cg,
            self.ct,
            self.range,
        )
    }
}

/// Owns a packed raw buffer and exposes it as `uhdr_raw_image`.
#[derive(Debug, Clone)]
pub struct OwnedPackedImage {
    buf: Vec<u8>,
    raw: sys::uhdr_raw_image,
}

impl OwnedPackedImage {
    /// Allocate a new packed buffer sized for `width`×`height` pixels in the given format.
    pub fn new(
        fmt: ImgFormat,
        width: u32,
        height: u32,
        cg: ColorGamut,
        ct: ColorTransfer,
        range: ColorRange,
    ) -> Result<Self> {
        let bpp = bytes_per_pixel(fmt)?;
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(bpp))
            .ok_or_else(|| Error::invalid_param("buffer size overflow"))?;
        let mut buf = vec![0u8; len];
        let mut planes = [ptr::null_mut(); 3];
        planes[0] = buf.as_mut_ptr() as *mut c_void;
        Ok(Self {
            buf,
            raw: sys::uhdr_raw_image {
                fmt,
                cg,
                ct,
                range,
                w: width,
                h: height,
                planes,
                stride: [width, 0, 0],
            },
        })
    }

    pub(crate) fn as_raw_mut(&mut self) -> &mut sys::uhdr_raw_image {
        // keep plane pointer up to date (in case of moves).
        self.raw.planes[0] = self.buf.as_mut_ptr() as *mut c_void;
        &mut self.raw
    }

    /// Mutable access to the backing pixel buffer.
    pub fn buffer(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Logical width in pixels.
    pub fn width(&self) -> u32 {
        self.raw.w
    }

    /// Logical height in pixels.
    pub fn height(&self) -> u32 {
        self.raw.h
    }

    /// Pixel layout.
    pub fn fmt(&self) -> ImgFormat {
        self.raw.fmt
    }

    /// Color metadata.
    pub fn meta(&self) -> (ColorGamut, ColorTransfer, ColorRange) {
        (self.raw.cg, self.raw.ct, self.raw.range)
    }
}

/// Borrowed view over packed pixels owned by a [`Decoder`].
pub struct DecodedPackedView<'a> {
    img: &'a mut sys::uhdr_raw_image,
    bpp: usize,
}

impl<'a> DecodedPackedView<'a> {
    pub(crate) fn new(img: &'a mut sys::uhdr_raw_image) -> Result<Self> {
        let bpp = bytes_per_pixel(img.fmt)?;
        Ok(Self { img, bpp })
    }

    /// Logical width in pixels.
    pub fn width(&self) -> u32 {
        self.img.w
    }

    /// Logical height in pixels.
    pub fn height(&self) -> u32 {
        self.img.h
    }

    /// Pixel layout of the view.
    pub fn fmt(&self) -> ImgFormat {
        self.img.fmt
    }

    /// Color metadata of the view.
    pub fn meta(&self) -> (ColorGamut, ColorTransfer, ColorRange) {
        (self.img.cg, self.img.ct, self.img.range)
    }

    /// Borrow a single packed row by index.
    pub fn row(&self, y: usize) -> Result<&'a [u8]> {
        let img: &sys::uhdr_raw_image = &*self.img;
        if y as u32 >= img.h {
            return Err(Error::invalid_param("row out of range"));
        }
        let plane_idx = sys::UHDR_PLANE_PACKED as usize;
        let stride_px = img.stride[plane_idx] as usize;
        let width_px = img.w as usize;
        if stride_px < width_px {
            return Err(Error::invalid_param("stride smaller than width"));
        }
        let stride_bytes = stride_px
            .checked_mul(self.bpp)
            .ok_or_else(|| Error::invalid_param("stride overflow"))?;
        let row_bytes = width_px
            .checked_mul(self.bpp)
            .ok_or_else(|| Error::invalid_param("row overflow"))?;
        if img.planes[plane_idx].is_null() {
            return Err(Error::invalid_param("null packed plane"));
        }
        // SAFETY: bounds checked above; plane is valid for lifetime 'a.
        let base = img.planes[plane_idx] as *const u8;
        let start = unsafe { base.add(y * stride_bytes) };
        let slice = unsafe { std::slice::from_raw_parts(start, row_bytes) };
        Ok(slice)
    }

    /// Override the color gamut metadata attached to this view.
    pub fn set_color_gamut(&mut self, cg: ColorGamut) {
        self.img.cg = cg;
    }

    /// Override the transfer-function metadata attached to this view.
    pub fn set_color_transfer(&mut self, ct: ColorTransfer) {
        self.img.ct = ct;
    }

    /// Override the range metadata attached to this view.
    pub fn set_color_range(&mut self, range: ColorRange) {
        self.img.range = range;
    }

    pub(crate) fn as_raw_mut(&mut self) -> &mut sys::uhdr_raw_image {
        self.img
    }

    /// Copy the pixels into an owned buffer, respecting stride.
    pub fn to_owned(&self) -> Result<DecodedPacked> {
        let img: &sys::uhdr_raw_image = &*self.img;
        let data = copy_raw_packed(img)?;
        let (cg, ct, range) = self.meta();
        Ok(DecodedPacked {
            fmt: img.fmt,
            cg,
            ct,
            range,
            width: img.w,
            height: img.h,
            data,
        })
    }
}

/// Parsed metadata describing an embedded gain map.
#[derive(Debug, Clone)]
pub struct GainMapMetadata {
    /// Maximum per-channel gain applied by the gain map.
    pub max_content_boost: [f32; 3],
    /// Minimum per-channel gain applied by the gain map.
    pub min_content_boost: [f32; 3],
    /// Per-channel gamma used to map base image to HDR.
    pub gamma: [f32; 3],
    /// Per-channel SDR offset.
    pub offset_sdr: [f32; 3],
    /// Per-channel HDR offset.
    pub offset_hdr: [f32; 3],
    /// Lower bound of the HDR capacity.
    pub hdr_capacity_min: f32,
    /// Upper bound of the HDR capacity.
    pub hdr_capacity_max: f32,
    /// Whether to reuse the base image color gamut for the gain map.
    pub use_base_cg: bool,
}

impl GainMapMetadata {
    pub(crate) fn from_sys(meta: &sys::uhdr_gainmap_metadata) -> Self {
        Self {
            max_content_boost: meta.max_content_boost,
            min_content_boost: meta.min_content_boost,
            gamma: meta.gamma,
            offset_sdr: meta.offset_sdr,
            offset_hdr: meta.offset_hdr,
            hdr_capacity_min: meta.hdr_capacity_min,
            hdr_capacity_max: meta.hdr_capacity_max,
            use_base_cg: meta.use_base_cg != 0,
        }
    }

    /// Target display peak brightness in nits (capacity * SDR reference white).
    pub fn target_display_peak_nits(&self) -> f32 {
        self.hdr_capacity_max * SDR_WHITE_NITS
    }
}

/// Borrowed descriptor over a caller-provided packed pixel buffer.
pub struct RawImage<'a> {
    pub(crate) inner: sys::uhdr_raw_image,
    _marker: PhantomData<&'a mut [u8]>,
}

impl<'a> RawImage<'a> {
    /// Create a packed descriptor for RGBA-like formats.
    pub fn packed(
        fmt: ImgFormat,
        width: u32,
        height: u32,
        data: &'a mut [u8],
        cg: ColorGamut,
        ct: ColorTransfer,
        range: ColorRange,
    ) -> Result<Self> {
        let bytes_per_pixel = bytes_per_pixel(fmt)?;
        let expected = width as usize * height as usize * bytes_per_pixel;
        if data.len() < expected {
            return Err(Error::invalid_param(
                "buffer smaller than width*height*bytes_per_pixel",
            ));
        }
        let mut planes = [ptr::null_mut(); 3];
        planes[0] = data.as_mut_ptr() as *mut c_void;
        Ok(Self {
            inner: sys::uhdr_raw_image {
                fmt,
                cg,
                ct,
                range,
                w: width,
                h: height,
                planes,
                stride: [width, 0, 0],
            },
            _marker: PhantomData,
        })
    }

    /// Create a packed RGBA8888 descriptor over the provided pixel buffer.
    pub fn rgba8888(
        width: u32,
        height: u32,
        data: &'a mut [u8],
        cg: ColorGamut,
        ct: ColorTransfer,
        range: ColorRange,
    ) -> Result<Self> {
        Self::packed(
            sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888,
            width,
            height,
            data,
            cg,
            ct,
            range,
        )
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::uhdr_raw_image {
        &mut self.inner
    }
}

impl<'a> RawImage<'a> {
    /// Logical width in pixels.
    pub fn width(&self) -> u32 {
        self.inner.w
    }

    /// Logical height in pixels.
    pub fn height(&self) -> u32 {
        self.inner.h
    }

    /// Pixel layout.
    pub fn fmt(&self) -> ImgFormat {
        self.inner.fmt
    }

    /// Color metadata.
    pub fn meta(&self) -> (ColorGamut, ColorTransfer, ColorRange) {
        (self.inner.cg, self.inner.ct, self.inner.range)
    }
}

/// Borrowed descriptor over a caller-provided compressed JPEG buffer.
pub struct CompressedImage<'a> {
    pub(crate) inner: sys::uhdr_compressed_image,
    _marker: PhantomData<&'a mut [u8]>,
}

impl<'a> CompressedImage<'a> {
    /// Wrap a mutable buffer containing JPEG bytes.
    pub fn from_bytes(
        data: &'a mut [u8],
        cg: ColorGamut,
        ct: ColorTransfer,
        range: ColorRange,
    ) -> Self {
        Self {
            inner: sys::uhdr_compressed_image {
                data: data.as_mut_ptr() as *mut c_void,
                data_sz: data.len(),
                capacity: data.len(),
                cg,
                ct,
                range,
            },
            _marker: PhantomData,
        }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::uhdr_compressed_image {
        &mut self.inner
    }
}

/// Copy a packed raw image plane into an owned Vec<u8>, honoring stride.
pub(crate) fn copy_raw_packed(img: &sys::uhdr_raw_image) -> Result<Vec<u8>> {
    let bytes_per_pixel = bytes_per_pixel(img.fmt)?;
    let plane_idx = sys::UHDR_PLANE_PACKED as usize;
    let data_ptr = img.planes[plane_idx];
    if data_ptr.is_null() {
        return Err(Error::invalid_param("null packed plane"));
    }
    let stride_px = img.stride[plane_idx] as usize;
    if stride_px == 0 {
        return Err(Error::invalid_param("zero stride"));
    }
    let width = img.w as usize;
    let height = img.h as usize;
    if stride_px < width {
        return Err(Error::invalid_param("stride smaller than width"));
    }

    let stride_bytes = stride_px
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| Error::invalid_param("stride overflow"))?;
    let row_bytes = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| Error::invalid_param("row overflow"))?;

    let mut out = vec![0u8; row_bytes * height];
    let mut src = data_ptr as *const u8;
    let mut dst = 0;
    for _ in 0..height {
        // SAFETY: bounds are validated above; src points into buffer provided by decoder.
        let row = unsafe { std::slice::from_raw_parts(src, stride_bytes) };
        out[dst..dst + row_bytes].copy_from_slice(&row[..row_bytes]);
        dst += row_bytes;
        src = unsafe { src.add(stride_bytes) };
    }
    Ok(out)
}

/// Copy a compressed image buffer into an owned Vec<u8>.
pub(crate) fn copy_compressed_image(img: &sys::uhdr_compressed_image) -> Result<Vec<u8>> {
    if img.data.is_null() {
        return Err(Error::invalid_param("null compressed data"));
    }
    let size = img.data_sz;
    if size > img.capacity {
        return Err(Error::invalid_param("compressed size exceeds capacity"));
    }
    // SAFETY: data/data_sz provided by encoder/decoder.
    let slice = unsafe { std::slice::from_raw_parts(img.data as *const u8, size) };
    Ok(slice.to_vec())
}

/// Bytes-per-pixel helper for the supported packed formats.
///
/// ```
/// use ultrahdr::{bytes_per_pixel, ImgFormat};
///
/// assert_eq!(
///     bytes_per_pixel(ImgFormat::UHDR_IMG_FMT_32bppRGBA8888).unwrap(),
///     4
/// );
/// ```
pub fn bytes_per_pixel(fmt: ImgFormat) -> Result<usize> {
    match fmt {
        sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888 => Ok(4),
        sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102 => Ok(4),
        sys::uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat => Ok(8),
        _ => Err(Error::invalid_param("unsupported packed format for helper")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_image_rgba_checks_buffer_size() {
        let mut buf = vec![0u8; 3];
        let res = RawImage::rgba8888(
            1,
            1,
            &mut buf,
            sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
            sys::uhdr_color_transfer::UHDR_CT_SRGB,
            sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
        );
        let err = res.err().expect("expected buffer size validation to fail");
        assert_eq!(err.code, sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM);
    }

    #[test]
    fn encoded_view_validates_backing_buffer() {
        // Null data pointer should be rejected.
        let img = sys::uhdr_compressed_image {
            data: std::ptr::null_mut(),
            data_sz: 4,
            capacity: 4,
            cg: sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
            ct: sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
            range: sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
        };
        let view = EncodedView::new(&img);
        let err = view.bytes().unwrap_err();
        assert_eq!(err.code, sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM);

        // data_sz larger than capacity should be rejected.
        let mut data = vec![1u8, 2, 3, 4];
        let img = sys::uhdr_compressed_image {
            data: data.as_mut_ptr() as *mut c_void,
            data_sz: 5,
            capacity: 4,
            cg: sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
            ct: sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
            range: sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
        };
        let view = EncodedView::new(&img);
        let err = view.bytes().unwrap_err();
        assert_eq!(err.code, sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM);
    }

    #[test]
    fn encoded_view_reads_slice() {
        let mut data = vec![1u8, 2, 3, 4];
        let img = sys::uhdr_compressed_image {
            data: data.as_mut_ptr() as *mut c_void,
            data_sz: data.len(),
            capacity: data.len(),
            cg: sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
            ct: sys::uhdr_color_transfer::UHDR_CT_PQ,
            range: sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
        };
        let view = EncodedView::new(&img);
        let bytes = view.bytes().unwrap();
        assert_eq!(bytes, &[1, 2, 3, 4]);
        let (cg, ct, range) = view.meta();
        assert_eq!(cg, sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3);
        assert_eq!(ct, sys::uhdr_color_transfer::UHDR_CT_PQ);
        assert_eq!(range, sys::uhdr_color_range::UHDR_CR_FULL_RANGE);
    }

    #[test]
    fn decoded_view_row_checks_bounds_and_stride() {
        let width = 2u32;
        let height = 2u32;
        let bpp = 4usize;
        let mut buf = vec![0u8; (width * height) as usize * bpp];
        let planes = [
            buf.as_mut_ptr() as *mut c_void,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ];
        let mut raw = sys::uhdr_raw_image {
            fmt: sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888,
            cg: sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
            ct: sys::uhdr_color_transfer::UHDR_CT_PQ,
            range: sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
            w: width,
            h: height,
            planes,
            stride: [1, 0, 0], // stride smaller than width triggers validation.
        };
        let view = DecodedPackedView::new(&mut raw).unwrap();
        let err = view.row(0).unwrap_err();
        assert_eq!(err.code, sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM);
    }

    #[test]
    fn decoded_view_to_owned_copies_packed_pixels() {
        let width = 2u32;
        let height = 2u32;
        let bpp = 4usize;
        // stride allows padding beyond logical width.
        let stride_px = 4usize;
        let mut buf = vec![0u8; stride_px * height as usize * bpp];
        // First row: pixels 1 and 2, then padding.
        buf[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        // Second row starts at stride offset.
        let row2_start = stride_px * bpp;
        buf[row2_start..row2_start + 8].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);

        let planes = [
            buf.as_mut_ptr() as *mut c_void,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ];
        let mut raw = sys::uhdr_raw_image {
            fmt: sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888,
            cg: sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
            ct: sys::uhdr_color_transfer::UHDR_CT_PQ,
            range: sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
            w: width,
            h: height,
            planes,
            stride: [stride_px as u32, 0, 0],
        };
        let view = DecodedPackedView::new(&mut raw).unwrap();
        let owned = view.to_owned().unwrap();
        assert_eq!(owned.data.len(), width as usize * height as usize * bpp);
        assert_eq!(&owned.data[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&owned.data[8..], &[9, 10, 11, 12, 13, 14, 15, 16]);
    }

    #[test]
    fn bytes_per_pixel_matches_supported_formats() {
        assert_eq!(
            bytes_per_pixel(sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888).unwrap(),
            4
        );
        assert_eq!(
            bytes_per_pixel(sys::uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat).unwrap(),
            8
        );
        let err = bytes_per_pixel(sys::uhdr_img_fmt::UHDR_IMG_FMT_UNSPECIFIED).unwrap_err();
        assert_eq!(err.code, sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM);
    }
}
