# pulp SIMD Optimization 实现计划

> **For Claude:** 必须使用 superpowers:executing-plans skill 按任务逐一实现本计划。

**目标：** 用 pulp crate 对 apply_gainmap_inner 的算术部分做 SIMD 向量化

**架构：** 将 process_row 重构为两遍处理：第一遍标量填充 SoA 缓冲区（LUT 查找 + gain sampling），第二遍用 pulp SIMD 批量处理算术运算。Feature-gated behind `simd` feature flag。

**技术栈：** pulp 0.22, Rust stable, optional feature flag

---

### 任务 1：添加 pulp 依赖和 feature flag

**文件：**
- 修改：`ultrahdr/Cargo.toml`

**步骤 1：修改 Cargo.toml**

在 `[features]` 添加 `simd = ["dep:pulp"]`，在 `[dependencies]` 添加 `pulp = { version = "0.22", optional = true }`

**步骤 2：验证编译**

运行：`CARGO_HOME=/tmp/cargo-home cargo build -p ultrahdr --features simd`

**步骤 3：提交**

---

### 任务 2：创建 SIMD 内核模块

**文件：**
- 创建：`ultrahdr/src/simd.rs`
- 修改：`ultrahdr/src/lib.rs`（添加 `#[cfg(feature = "simd")] mod simd;`）

核心函数：

1. `apply_gain_simd(r_lin, g_lin, b_lin, factor_r, factor_g, factor_b, offset_sdr, offset_hdr, hdr_r, hdr_g, hdr_b)` — 用 pulp WithSimd 实现 `(lin + offset_sdr) * factor - offset_hdr`，按通道处理，head/tail 分离
2. `clamp_simd(data, min, max)` — 用 pulp 的 min_f32s/max_f32s 批量 clamp

测试：在 simd.rs 底部写 3 个单元测试验证 SIMD == scalar（用非对齐长度如 n=37 测试 tail 处理）

---

### 任务 3：将 SIMD 集成到 apply_gainmap_inner

**文件：**
- 修改：`ultrahdr/src/decoder.rs:348-632`
- 修改：`ultrahdr/src/gainmap/math.rs`（GainLut 添加 pub accessor）
- 创建：`ultrahdr/tests/simd_equivalence.rs`

**关键修改：**

1. `GainLut` 添加公开方法：`pub fn gain_factor_pub(&self, gain: f32, ch: usize) -> f32`、`pub fn offset_sdr(&self) -> [f32; 3]`、`pub fn offset_hdr(&self) -> [f32; 3]`

2. `process_row` 闭包用 `#[cfg(feature = "simd")]` 分支：
   - Pass 1 (标量): 遍历行内所有像素，填充 SoA 缓冲区：r_lin_buf, g_lin_buf, b_lin_buf, a_buf, factor_r_buf, factor_g_buf, factor_b_buf。sRGB LUT、gain sampling、gain factor LUT 全部标量。
   - Pass 2 (SIMD): `apply_gain_simd()` 批量计算 `(lin + offset) * factor - offset`
   - Pass 3: transfer function — Linear 用 `clamp_simd()`，PQ/HLG 标量 LUT，sRGB 标量
   - Pass 4: output format conversion — 标量写出（Rgba8888/1010102/F16）
   - `#[cfg(not(feature = "simd"))]` 保留原始标量路径不变

3. `Arch::new()` 在 process_row 外调用一次，通过闭包捕获传入

4. simd_equivalence.rs：端到端测试，用 64x64 gradient 图片，验证所有 transfer×format 组合输出确定性

**注意：** vec 分配每行 7 × width × 4 bytes ≈ 14KB (512 宽)，L1 缓存内。

---

### 任务 4：性能验证和收尾

**步骤：**

1. 运行 decode_profile（无 simd）和（有 simd），对比 apply_gainmap 时间
2. 运行全量测试：`cargo test -p ultrahdr --features simd,rayon`
3. 运行 bit_exact 测试：`cargo test --release --features simd,rayon --test bit_exact -- --ignored`
4. clippy + fmt 检查
5. 最终提交，汇报性能对比结果

---

### 关键注意事项

- **bit-exact 保证**：IEEE 754 下 f32 的 add/mul/sub 是确定性的，SIMD 与标量结果完全一致
- **Arch::new()** 行循环外调用一次，避免每行重检测 CPU
- **gain sampling** 保持标量（bilinear/IDW 数据依赖访问无法向量化）
- **LUT 查找** 保持标量（pulp 不暴露 gather）
- 预期收益保守 1.2-1.3x（SIMD 只覆盖算术部分约 30%）
