# Test fixtures

`plain.jpg` is `tests/data/minnie-320x240-yuv.jpg` from upstream
[libultrahdr](https://github.com/google/libultrahdr) (Apache-2.0), copied here so the test suite
does not depend on the `libultrahdr-sys` submodule layout. It is a plain baseline JPEG with no gain
map, used for the negative `is_uhdr_image` / `probe` cases. All other fixtures are synthesised at
runtime by `../api.rs`.
