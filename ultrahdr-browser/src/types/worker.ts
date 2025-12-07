export type WorkerStatus =
  | { type: "status"; payload: string }
  | { type: "stdout"; payload: string }
  | { type: "stderr"; payload: string }
  | { type: "done"; payload: { fileName: string; buffer: ArrayBuffer } }
  | { type: "error"; payload: string };

export type InFile = {
  name: string;
  buffer: ArrayBuffer;
};

export type BakeRequest = {
  type: "bake";
  files: InFile[];
  opts: {
    outName: string;
    baseQ: number;
    gainmapQ: number;
    scale: number;
    multichannel: boolean;
    targetPeak?: number;
  };
};

export type MotionRequest = {
  type: "motion";
  files: InFile[];
  opts: {
    outName: string;
    timestampUs?: number;
  };
};

export type WorkerRequest = BakeRequest | MotionRequest;
