import {
  WASI,
  File,
  Directory,
  OpenFile,
  PreopenDirectory,
  ConsoleStdout,
} from "@bjorn3/browser_wasi_shim";

const hdrInput = document.getElementById("hdr-file");
const sdrInput = document.getElementById("sdr-file");
const runBtn = document.getElementById("run-btn");
const statusEl = document.getElementById("status");
const resultEl = document.getElementById("result");
const dlLink = document.getElementById("download-link");
const previewImg = document.getElementById("preview-img");

const rootDir = new Directory(new Map());

async function fileToUint8(file) {
  const buf = await file.arrayBuffer();
  return new Uint8Array(buf);
}

function setStatus(msg) {
  statusEl.textContent = msg;
}

function resetFs() {
  rootDir.contents = new Map();
}

function writeFile(path, data) {
  rootDir.contents.set(path, new File(data, { readonly: false }));
}

function readFile(path) {
  const entry = rootDir.contents.get(path);
  if (!entry || !(entry instanceof File)) return null;
  return entry.data;
}

async function bake() {
  resultEl.classList.add("hidden");
  setStatus("Preparing files…");

  const hdrFile = hdrInput.files?.[0];
  const sdrFile = sdrInput.files?.[0];
  if (!hdrFile || !sdrFile) {
    setStatus("Please choose both HDR and SDR JPEGs.");
    return;
  }

  resetFs();
  writeFile("hdr.jpg", await fileToUint8(hdrFile));
  writeFile("sdr.jpg", await fileToUint8(sdrFile));

  const args = [
    "ultrahdr-bake.wasm",
    "--out",
    "out.jpg",
    "hdr.jpg",
    "sdr.jpg",
  ];

  const fds = [
    new OpenFile(new File(new Uint8Array())),
    ConsoleStdout.lineBuffered((line) => console.log(line)),
    ConsoleStdout.lineBuffered((line) => console.error(line)),
    new PreopenDirectory("", rootDir.contents),
  ];

  // WASI signature: new WASI(args, env, fds, options?)
  const wasi = new WASI(args, [], fds, {});

  setStatus("Fetching wasm binary…");
  const wasmBytes = await fetch("/ultrahdr-bake.wasm").then((r) => r.arrayBuffer());

  setStatus("Running ultrahdr-bake inside WASI…");
  const { instance } = await WebAssembly.instantiate(wasmBytes, {
    wasi_snapshot_preview1: wasi.wasiImport,
    // The WASI build may import setjmp/longjmp; stub for browsers.
    env: {
      setjmp: () => 0,
      longjmp: () => {
        throw new Error("longjmp called");
      },
    },
  });
  wasi.start(instance);

  setStatus("Reading output…");
  const outBytes = readFile("out.jpg");
  if (!outBytes) {
    throw new Error("Output file missing");
  }
  const blob = new Blob([outBytes], { type: "image/jpeg" });
  const url = URL.createObjectURL(blob);
  dlLink.href = url;
  previewImg.src = url;

  resultEl.classList.remove("hidden");
  setStatus("Done.");
}

runBtn.addEventListener("click", () => {
  bake().catch((err) => {
    console.error(err);
    setStatus(`Error: ${err.message}`);
  });
});
