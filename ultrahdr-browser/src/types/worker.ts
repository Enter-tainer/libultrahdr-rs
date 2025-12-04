export type WorkerStatus =
  | { type: "status"; payload: string }
  | { type: "stdout"; payload: string }
  | { type: "stderr"; payload: string }
  | { type: "done"; payload: { fileName: string; buffer: ArrayBuffer } }
  | { type: "error"; payload: string };

export type BakeRequest = {
  type: "bake";
  hdr: ArrayBuffer;
  sdr: ArrayBuffer;
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
  photo: ArrayBuffer;
  video: ArrayBuffer;
  opts: {
    outName: string;
    timestampUs?: number;
  };
};

export type WorkerRequest = BakeRequest | MotionRequest;
