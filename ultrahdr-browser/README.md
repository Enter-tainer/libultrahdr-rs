# UltraHDR browser demo (WASI)

Minimal example that runs the `ultrahdr-bake` WASI binary in the browser using `@bjorn3/browser_wasi_shim`.
Users can upload an HDR (gain-map) JPEG and an SDR JPEG, and the wasm binary produces an UltraHDR JPEG entirely client-side.

## Prereqs
- Node 18+ / pnpm
- Build the WASI binary: `cargo build --target wasm32-wasip1 -p ultrahdr-bake --release`

## Setup
1. Install deps (requires network):
   ```bash
   pnpm install
   ```
2. Build wasm binary if not present:
   ```bash
   cargo build --target wasm32-wasip1 -p ultrahdr-bake --release
   ```
3. Copy wasm + run locally (copy is automatic in scripts):
   ```bash
   pnpm dev      # copies wasm then starts Vite
   # or
   pnpm build    # copies wasm then builds static assets
   ```
   Open the URL Vite prints, choose two JPEGs, and download the baked output.

Notes:
- The WASI build currently imports `setjmp/longjmp`; the demo stubs those imports in JS.
- If you see fs errors in the browser, ensure `@bjorn3/browser_wasi_shim` is installed and the wasm file is present at `/ultrahdr-bake.wasm`.
- `pnpm wasm:copy` copies the built wasm to `public/ultrahdr-bake.wasm` (ignored in git).
