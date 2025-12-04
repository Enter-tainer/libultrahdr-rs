import {
  WASI,
  File,
  Directory,
  OpenFile,
  PreopenDirectory,
  ConsoleStdout,
} from "@bjorn3/browser_wasi_shim";
import type { WorkerRequest, WorkerStatus } from "./types/worker";

const rootDir = new Directory(new Map());

function resetFs() {
  rootDir.contents = new Map();
}

function writeFile(path: string, data: Uint8Array) {
  rootDir.contents.set(path, new File(data, { readonly: false }));
}

function readFile(path: string) {
  const entry = rootDir.contents.get(path);
  if (!entry || !(entry instanceof File)) return null;
  return entry.data;
}

function emit(msg: WorkerStatus) {
  postMessage(msg);
}

async function runBake(req: Extract<WorkerRequest, { type: "bake" }>) {
  emit({ type: "status", payload: "Preparing WASI FS…" });
  resetFs();
  writeFile("input1.jpg", new Uint8Array(req.hdr));
  writeFile("input2.jpg", new Uint8Array(req.sdr));

  const args = [
    "ultrahdr-bake.wasm",
    "--out",
    req.opts.outName,
    "--base-q",
    req.opts.baseQ.toString(),
    "--gm-q",
    req.opts.gainmapQ.toString(),
    "--scale",
    req.opts.scale.toString(),
  ];
  if (req.opts.multichannel) args.push("--multichannel");
  if (req.opts.targetPeak !== undefined) {
    args.push("--target-peak", req.opts.targetPeak.toString());
  }
  // Always rely on CLI auto-detection: provide two positional inputs.
  args.push("input1.jpg", "input2.jpg");

  await runCli(args, req.opts.outName);
}

async function runMotion(req: Extract<WorkerRequest, { type: "motion" }>) {
  emit({ type: "status", payload: "Preparing WASI FS…" });
  resetFs();
  writeFile("photo.jpg", new Uint8Array(req.photo));
  writeFile("video.mp4", new Uint8Array(req.video));

  const args = [
    "ultrahdr-bake.wasm",
    "motion",
    "--photo",
    "photo.jpg",
    "--video",
    "video.mp4",
    "--out",
    req.opts.outName,
  ];
  if (req.opts.timestampUs !== undefined) {
    args.push("--timestamp-us", req.opts.timestampUs.toString());
  }

  await runCli(args, req.opts.outName);
}

async function runCli(args: string[], outName: string) {
  const fds = [
    new OpenFile(new File(new Uint8Array())),
    ConsoleStdout.lineBuffered((line) => emit({ type: "stdout", payload: line })),
    ConsoleStdout.lineBuffered((line) => emit({ type: "stderr", payload: line })),
    new PreopenDirectory("", rootDir.contents),
  ];
  const wasi = new WASI(args, [], fds, {});

  emit({ type: "status", payload: "Fetching wasm…" });
  const wasmBytes = await fetch("/ultrahdr-bake.wasm").then((r) => r.arrayBuffer());

  emit({ type: "status", payload: "Running ultrahdr-bake…" });
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

  const outBytes = readFile(outName);
  if (!outBytes) {
    throw new Error("Output file missing");
  }
  emit({
    type: "done",
    payload: { fileName: outName, buffer: outBytes.buffer },
  });
}

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const req = event.data;
  if (req.type === "bake") {
    runBake(req).catch((err) => emit({ type: "error", payload: err.message || String(err) }));
  } else if (req.type === "motion") {
    runMotion(req).catch((err) => emit({ type: "error", payload: err.message || String(err) }));
  }
};
