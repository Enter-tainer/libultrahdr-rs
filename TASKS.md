# Byte-Exact Metadata 实现计划

> **For Claude:** 必须使用 superpowers:executing-plans skill 按任务逐一实现本计划。

**目标：** 使 Rust encoder 输出的 metadata segments (XMP, ISO, MPF) 与 C++ 字节级一致

**架构：** 修改 `encoder.rs` (XMP+assembly), `gainmap/metadata.rs` (fraction conversion), 新增 `bitexact.rs` 对比测试

**技术栈：** Rust, cargo test, ultrahdr-sys FFI

---

## 关键发现

| 差异 | C++ | Rust | 严重度 |
|------|-----|------|--------|
| Primary XMP | Container directory (列出 primary+gainmap) | Gainmap metadata (错误位置!) | P0 |
| Secondary XMP | Gainmap metadata 值 | 无 | P0 |
| Float→Fraction | Continued fractions (最优有理逼近) | 固定 denom=10000 + truncation | P1 |
| XMP 格式 | XmlWriter (带缩进、xmlns、x:xmptk) | 手动 string format | P1 |
| Secondary segment | SOI→XMP APP1→ISO APP2→rest | SOI→ISO APP2→rest (无 XMP) | P0 |

---

### Task 1: 添加 metadata 字节对比诊断测试

**文件:** `ultrahdr/tests/bitexact.rs`

**目的:** 新增测试，对同一 input 用 Rust 和 C++ encode，提取并对比所有 metadata segments 的原始字节。诊断测试，先看清所有差异。

需要实现辅助函数来解析 JPEG segments，提取 XMP APP1、ISO APP2、MPF APP2。对比 primary 和 secondary 中的所有 metadata。

**验证:** `CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test bitexact metadata_bytes_diagnostic -- --nocapture`

**期望:** 测试失败但输出详细差异。

**agent_id:** meta-exact

---

### Task 2: 修复 XMP 结构

**文件:**
- `ultrahdr/src/gainmap/metadata.rs` — 新增 `write_xmp_primary_container(secondary_image_size, metadata_version)`
- `ultrahdr/src/encoder.rs` — `assemble_ultrahdr()` 中 primary 用 container XMP, secondary 插入 gainmap metadata XMP

**关键变更:**

1. **Primary XMP** → Container directory，参考 C++ `generateXmpForPrimaryImage()` (jpegrutils.cpp:636-673):
   - `x:xmpmeta` 包含 `x:xmptk="Adobe XMP Core 5.1.2"`
   - `rdf:RDF` → `rdf:Description` 带 xmlns for Container, Item, hdrgm
   - `Container:Directory` → `rdf:Seq` → 两个 `rdf:li` (Primary + GainMap)
   - GainMap item 有 `Item:Length` = secondary_image_size

2. **Secondary XMP** → Gainmap metadata，参考 C++ `generateXmpForSecondaryImage()` (jpegrutils.cpp:675-699):
   - 包含 `x:xmptk="Adobe XMP Core 5.1.2"`
   - hdrgm:Version, GainMapMin, GainMapMax, Gamma, OffsetSDR, OffsetHDR, HDRCapacityMin, HDRCapacityMax, BaseRenditionIsHDR
   - 用 C++ 的 XmlWriter 生成格式（或精确匹配其输出）

3. **Assembly** 修改:
   - Primary: 插入 container XMP APP1
   - Secondary: SOI → XMP APP1 (gainmap metadata) → ISO APP2 (full) → rest of JPEG

**验证:** 重新运行 Task 1 诊断测试

**agent_id:** meta-exact

---

### Task 3: 修复 float→fraction — continued fractions 算法

**文件:**
- `ultrahdr/src/gainmap/metadata.rs` — 新增 `float_to_unsigned_fraction()`, `float_to_signed_fraction()`
- `ultrahdr/src/encoder.rs` — `metadata_to_frac()` 使用新函数

**关键变更:**

移植 C++ `floatToUnsignedFractionImpl()` (gainmapmath.cpp:1626-1680):
- 使用 continued fractions 找最优有理逼近
- maxNumerator = UINT32_MAX (unsigned) / INT32_MAX (signed)
- 处理负数：取绝对值转换后恢复符号

修改 `metadata_to_frac()` 不再用固定 denom=10000，每个字段调用 continued fractions。

**验证:** ISO binary metadata 字节匹配 C++

**agent_id:** meta-exact

---

### Task 4: 验证 MPF、segment 顺序、最终对比

**文件:** `ultrahdr/src/encoder.rs` (如需调整)

**检查项:**
1. MPF 字节对比
2. APP segment 顺序: EXIF APP1 → XMP APP1 → ISO APP2 stub → ICC APP2 → MPF APP2
3. Secondary 顺序: SOI → XMP APP1 → ISO APP2 full → rest

**验证:**
```bash
cargo fmt -p ultrahdr -- --check
cargo clippy -p ultrahdr --all-targets -- -D warnings
cargo test -p ultrahdr
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test bitexact -- --nocapture
```

**期望:** 所有测试通过，metadata 字节对比零差异（uniform-RGB 场景）

**agent_id:** meta-exact
