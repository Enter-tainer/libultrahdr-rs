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

function readFile(path: string): Uint8Array | null {
  const entry = rootDir.contents.get(path);
  if (!entry || !(entry instanceof File)) return null;
  return entry.data as Uint8Array;
}

function emit(msg: WorkerStatus) {
  postMessage(msg);
}

const SAFE_BAKE_NAMES = ["input1.jpg", "input2.jpg"];
const SAFE_MOTION_NAMES = ["motion1.bin", "motion2.bin"];

async function runBake(req: Extract<WorkerRequest, { type: "bake" }>) {
  emit({ type: "status", payload: "Preparing WASI FS…" });
  resetFs();
  if (req.files.length !== 2) {
    throw new Error("Need exactly two JPEG inputs");
  }
  const files = req.files.slice(0, 2);
  for (let i = 0; i < files.length; i++) {
    writeFile(SAFE_BAKE_NAMES[i], new Uint8Array(files[i].buffer));
  }

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
  args.push(SAFE_BAKE_NAMES[0], SAFE_BAKE_NAMES[1]);

  await runCli(args, req.opts.outName);
}

async function runMotion(req: Extract<WorkerRequest, { type: "motion" }>) {
  emit({ type: "status", payload: "Preparing WASI FS…" });
  resetFs();
  if (req.files.length !== 2) {
    throw new Error("Need exactly two inputs (photo + video)");
  }
  const files = req.files.slice(0, 2);
  for (let i = 0; i < files.length; i++) {
    writeFile(SAFE_MOTION_NAMES[i], new Uint8Array(files[i].buffer));
  }

  const args = ["ultrahdr-bake.wasm", "motion", "--out", req.opts.outName];
  if (req.opts.timestampUs !== undefined) {
    args.push("--timestamp-us", req.opts.timestampUs.toString());
  }
  args.push(SAFE_MOTION_NAMES[0], SAFE_MOTION_NAMES[1]);

  await runCli(args, req.opts.outName);
}

async function runCli(args: string[], outName: string) {
  const fds = [
    new OpenFile(new File(new Uint8Array())),
    ConsoleStdout.lineBuffered((line) =>
      emit({ type: "stdout", payload: line }),
    ),
    ConsoleStdout.lineBuffered((line) =>
      emit({ type: "stderr", payload: line }),
    ),
    new PreopenDirectory("", rootDir.contents),
  ];
  const wasi = new WASI(args, [], fds, {});

  emit({ type: "status", payload: "Fetching wasm…" });
  const baseOrigin =
    (self as unknown as { location?: Location }).location?.origin ||
    "http://localhost";
  const wasmUrl = new URL(
    `${import.meta.env.BASE_URL || "/"}ultrahdr-bake.wasm`,
    baseOrigin,
  ).toString();
  const wasmBytes = await fetch(wasmUrl).then((r) => r.arrayBuffer());

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
  wasi.start(
    instance as unknown as {
      exports: { memory: WebAssembly.Memory; _start: () => unknown };
    },
  );

  const outBytes = readFile(outName);
  if (!outBytes) {
    throw new Error("Output file missing");
  }
  const outCopy = new Uint8Array(outBytes.byteLength);
  outCopy.set(outBytes);
  emit({
    type: "done",
    payload: {
      fileName: outName,
      buffer: outCopy.buffer,
    },
  });
}

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const req = event.data;
  if (req.type === "bake") {
    runBake(req).catch((err) =>
      emit({ type: "error", payload: err.message || String(err) }),
    );
  } else if (req.type === "motion") {
    runMotion(req).catch((err) =>
      emit({ type: "error", payload: err.message || String(err) }),
    );
  }
};
