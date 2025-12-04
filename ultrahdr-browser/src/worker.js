import {
  WASI,
  File,
  Directory,
  OpenFile,
  PreopenDirectory,
  ConsoleStdout,
} from "@bjorn3/browser_wasi_shim";

const rootDir = new Directory(new Map());

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

async function runBake(hdrBytes, sdrBytes) {
  postMessage({ type: "status", payload: "Preparing WASI FS…" });
  resetFs();
  writeFile("hdr.jpg", new Uint8Array(hdrBytes));
  writeFile("sdr.jpg", new Uint8Array(sdrBytes));

  const args = ["ultrahdr-bake.wasm", "--out", "out.jpg", "hdr.jpg", "sdr.jpg"];
  const fds = [
    new OpenFile(new File(new Uint8Array())),
    ConsoleStdout.lineBuffered((line) => console.log(line)),
    ConsoleStdout.lineBuffered((line) => console.error(line)),
    new PreopenDirectory("", rootDir.contents),
  ];
  const wasi = new WASI(args, [], fds, {});

  postMessage({ type: "status", payload: "Fetching wasm…" });
  const wasmBytes = await fetch("/ultrahdr-bake.wasm").then((r) => r.arrayBuffer());

  postMessage({ type: "status", payload: "Running ultrahdr-bake…" });
  const { instance } = await WebAssembly.instantiate(wasmBytes, {
    wasi_snapshot_preview1: wasi.wasiImport,
    env: {
      setjmp: () => 0,
      longjmp: () => {
        throw new Error("longjmp called");
      },
    },
  });
  wasi.start(instance);

  const outBytes = readFile("out.jpg");
  if (!outBytes) {
    throw new Error("Output file missing");
  }
  postMessage({ type: "done", payload: outBytes.buffer }, [outBytes.buffer]);
}

self.onmessage = (event) => {
  const { hdr, sdr } = event.data;
  runBake(hdr, sdr).catch((err) => {
    console.error(err);
    postMessage({ type: "error", payload: err.message || String(err) });
  });
};
