# GOAL: RIIR libultrahdr — 纯 Rust 实现 UltraHDR Gain Map JPEG

## 背景

当前 `libultrahdr-rs` 是 Google libultrahdr C++ 库的 Rust FFI 封装。目标是将其重写为纯 Rust 实现（RIIR），去除 C/C++ 依赖，同时保持所有功能不变，API 改为 Rust best practice 风格。

## 核心决策

1. **JPEG 编解码**：使用现有 Rust crate（`jpeg-decoder`、`mozjpeg`/`zune-jpeg`），不从零实现
2. **Crate 结构**：合并为单一 `ultrahdr` crate + 独立 `ultrahdr-bake` CLI，去掉 `ultrahdr-sys`
3. **SIMD 优化**：先实现纯 Rust 正确性版本，后续用 benchmark 定位热点再针对性 SIMD 优化

## 功能列表

### 编码（Encoder）
- 接受 HDR raw 像素 + SDR compressed JPEG，计算 gain map，输出 UltraHDR JPEG
- 接受 HDR raw only，自动 tone map 生成 SDR（API-0 场景）
- 支持多通道/单通道 gain map
- 可配置：JPEG quality、gainmap scale、gamma、target peak brightness、encoder preset
- 输出格式：UltraHDR JPEG（含嵌入 gain map）

### 解码（Decoder）
- 解码 UltraHDR JPEG 为 packed 像素缓冲区
- 支持输出格式：RGBA8888 (SDR)、RGBA1010102 (HDR PQ/HLG)、RGBAF16 (HDR linear)
- 探测 gain map 元数据（无需完整解码）
- 可配置最大 display boost

### 核心算法
- Gain map 数学：HDR/SDR 比值计算、gain map 生成与应用
- Tone mapping：HDR → SDR 自动映射
- 色彩空间转换：sRGB ↔ linear、PQ ↔ linear、HLG ↔ linear
- 色域矩阵转换：BT.709 ↔ Display P3 ↔ BT.2100
- ICC profile 解析（色域检测）

### 元数据
- ISO 21496-1 gain map metadata 读写
- XMP 元数据嵌入/提取
- Multi-Picture Format (MPF) 组装

### CLI（ultrahdr-bake）
- 保持现有功能：HDR+SDR → UltraHDR 合成
- 保持现有功能：Motion Photo 组装
- 保持现有功能：自动检测 HDR/SDR 输入
- 适配新的纯 Rust ultrahdr API

## Crate 结构

```
ultrahdr/
├── src/
│   ├── lib.rs
│   ├── encoder.rs          # 高层编码器（builder pattern）
│   ├── decoder.rs          # 高层解码器
│   ├── gainmap/
│   │   ├── mod.rs
│   │   ├── math.rs         # gain map 计算
│   │   └── metadata.rs     # ISO 21496-1 / XMP 元数据
│   ├── color/
│   │   ├── mod.rs
│   │   ├── gamut.rs        # 色域转换矩阵
│   │   ├── transfer.rs     # 传输函数（PQ, HLG, sRGB, linear）
│   │   └── icc.rs          # ICC profile 解析
│   ├── jpeg/
│   │   ├── mod.rs
│   │   ├── decode.rs       # JPEG 解码（封装 jpeg-decoder）
│   │   └── encode.rs       # JPEG 编码（封装 mozjpeg/zune-jpeg）
│   ├── mpf.rs              # Multi-Picture Format
│   ├── types.rs            # 核心类型（PixelFormat, ColorGamut, ...）
│   └── error.rs            # 错误类型
ultrahdr-bake/              # CLI（独立 crate，依赖 ultrahdr）
```

## API 设计原则

- 原生 Rust enum 取代 C type alias（类型安全，exhaustive match）
- Builder pattern 编码器（编译期约束必填参数）
- 纯安全 Rust（无 unsafe，无 raw pointer）
- 标准 trait 实现（Debug, Clone, PartialEq 等）
- 返回 `Result<T, ultrahdr::Error>`

## 依赖

- `jpeg-decoder` — JPEG 解码
- `mozjpeg` 或 `zune-jpeg` — JPEG 编码
- `quick-xml` — XMP 元数据解析
- `img-parts` — JPEG segment 操作
- `memchr` — 高效字节搜索
- `insta` (dev) — snapshot 测试

## 约束

- 不引入 C/C++ 编译依赖（纯 Rust，可能的例外：mozjpeg 自身有 C 依赖，如选择则需评估替代方案）
- 保持与现有 `ultrahdr-bake` CLI 的功能兼容
- 保持 Apache-2.0 许可证
- 支持 Linux/macOS/Windows，保留 WASM 目标可行性

## 测试策略

- **单元测试**：每个数学函数独立测试（色彩转换精度、gain map 计算正确性）
- **Snapshot 测试**：使用 `cargo insta` 对编解码输出做 snapshot 对照
- **集成测试**：已知 UltraHDR JPEG 样本的 encode→decode round-trip
- **对照测试**：C++ 版本输出作为 golden reference，确保 tolerance 内一致
- **Property-based 测试**：encode(decode(x)) ≈ x（有损容差内）

## 验收标准（Success Criteria）

1. `cargo build` 无需 cmake/nasm/pkg-config，纯 Rust 编译通过
2. `cargo test` 全部通过，包含上述所有测试类型
3. `ultrahdr-bake` CLI 保持所有现有功能（encode、motion photo、auto-detect）
4. 对照 C++ 版本的 UltraHDR JPEG 输出，像素差异在可接受容差内
5. `cargo clippy` 零警告
6. 无 unsafe 代码（JPEG crate 内部的 unsafe 不计）
7. API 文档完整（`cargo doc` 无警告）
