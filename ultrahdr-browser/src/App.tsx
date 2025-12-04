import React from "react";
import { Upload, Wand2, Film } from "lucide-react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./components/ui/tabs";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui/card";
import { Label } from "./components/ui/label";
import { Input } from "./components/ui/input";
import { Button } from "./components/ui/button";
import { Textarea } from "./components/ui/textarea";
import { Select } from "./components/ui/select";
import type { WorkerStatus } from "./types/worker";

type Mode = "bake" | "motion";

const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });

export default function App() {
  const [mode, setMode] = React.useState<Mode>("bake");
  const [status, setStatus] = React.useState("Idle");
  const [log, setLog] = React.useState<string[]>([]);
  const [outputUrl, setOutputUrl] = React.useState<string | null>(null);
  const [outputName, setOutputName] = React.useState<string | null>(null);
  const [previews, setPreviews] = React.useState<{ a?: string; b?: string }>({});

  const [bakeInputs, setBakeInputs] = React.useState({
    hdr: null as File | null,
    sdr: null as File | null,
    baseQ: 95,
    gainmapQ: 95,
    scale: 1,
    multichannel: false,
    targetPeak: "",
  });
  const [motionInputs, setMotionInputs] = React.useState({
    photo: null as File | null,
    video: null as File | null,
    timestampUs: "",
  });

  React.useEffect(() => {
    worker.onmessage = (event: MessageEvent<WorkerStatus>) => {
      const { type, payload } = event.data;
      if (type === "status") {
        setStatus(payload);
        setLog((prev) => [...prev, payload]);
      } else if (type === "stdout") {
        setLog((prev) => [...prev, payload]);
      } else if (type === "stderr") {
        setLog((prev) => [...prev, `[err] ${payload}`]);
      } else if (type === "done") {
        const blob = new Blob([payload.buffer], { type: "image/jpeg" });
        const url = URL.createObjectURL(blob);
        setOutputUrl(url);
        setOutputName(payload.fileName);
        setStatus("Done");
        setLog((prev) => [...prev, `Wrote ${payload.fileName}`]);
      } else if (type === "error") {
        setStatus("Error");
        setLog((prev) => [...prev, `[error] ${payload}`]);
      }
    };
  }, []);

  const handleBake = async () => {
    if (!bakeInputs.hdr || !bakeInputs.sdr) {
      setStatus("Please choose two JPEG files");
      return;
    }
    setOutputUrl(null);
    setLog([]);
    setStatus("Preparing…");
    const hdrBuf = await bakeInputs.hdr.arrayBuffer();
    const sdrBuf = await bakeInputs.sdr.arrayBuffer();
    worker.postMessage(
      {
        type: "bake",
        hdr: hdrBuf,
        sdr: sdrBuf,
        opts: {
          outName: "ultrahdr_bake_out.jpg",
          baseQ: bakeInputs.baseQ,
          gainmapQ: bakeInputs.gainmapQ,
          scale: bakeInputs.scale,
          multichannel: bakeInputs.multichannel,
          targetPeak: bakeInputs.targetPeak ? Number(bakeInputs.targetPeak) : undefined,
        },
      },
      [hdrBuf, sdrBuf]
    );
  };

  const handleMotion = async () => {
    if (!motionInputs.photo || !motionInputs.video) {
      setStatus("Please choose both photo and video");
      return;
    }
    setOutputUrl(null);
    setLog([]);
    setStatus("Preparing…");
    const photoBuf = await motionInputs.photo.arrayBuffer();
    const videoBuf = await motionInputs.video.arrayBuffer();
    worker.postMessage(
      {
        type: "motion",
        photo: photoBuf,
        video: videoBuf,
        opts: {
          outName: "motionphoto.jpg",
          timestampUs: motionInputs.timestampUs ? Number(motionInputs.timestampUs) : undefined,
        },
      },
      [photoBuf, videoBuf]
    );
  };

  return (
    <div className="mx-auto flex min-h-screen max-w-6xl flex-col gap-6 px-6 py-10">
      <header className="flex items-center justify-between">
        <div>
          <p className="text-sm uppercase tracking-[0.3em] text-slate-400">UltraHDR</p>
          <h1 className="text-3xl font-semibold text-white">WASI Browser Studio</h1>
          <p className="text-slate-400">
            Encode UltraHDR JPEGs or motion photos directly in your browser via WASI.
          </p>
        </div>
        <div className="hidden md:block rounded-xl border border-slate-800 bg-slate-900/50 px-4 py-3 text-sm text-slate-300">
          <p>Status: {status}</p>
          <p className="text-xs text-slate-500">Uses @bjorn3/browser_wasi_shim</p>
        </div>
      </header>

      <Tabs defaultValue="bake" className="w-full" >
        <TabsList className="bg-slate-900">
          <TabsTrigger value="bake" onClick={() => setMode("bake")}>
            <Wand2 className="mr-2 h-4 w-4" />
            UltraHDR Bake
          </TabsTrigger>
          <TabsTrigger value="motion" onClick={() => setMode("motion")}>
            <Film className="mr-2 h-4 w-4" />
            Motion Photo
          </TabsTrigger>
        </TabsList>

        <TabsContent value="bake">
          <Card>
            <CardHeader>
              <CardTitle>HDR + SDR ➜ UltraHDR</CardTitle>
              <CardDescription>Auto-detects HDR/SDR ordering; tweak quality knobs if needed.</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              <div className="space-y-3">
                <Label className="flex items-center gap-2">
                  <Upload className="h-4 w-4" /> Input JPEG A
                </Label>
                <Input
                  type="file"
                  accept="image/jpeg"
                  onChange={(e) => {
                    const file = e.target.files?.[0] || null;
                    setBakeInputs((s) => ({ ...s, hdr: file }));
                    if (file) {
                      const url = URL.createObjectURL(file);
                      setPreviews((p) => ({ ...p, a: url }));
                    }
                  }}
                />
                <Label className="flex items-center gap-2">
                  <Upload className="h-4 w-4" /> Input JPEG B
                </Label>
                <Input
                  type="file"
                  accept="image/jpeg"
                  onChange={(e) => {
                    const file = e.target.files?.[0] || null;
                    setBakeInputs((s) => ({ ...s, sdr: file }));
                    if (file) {
                      const url = URL.createObjectURL(file);
                      setPreviews((p) => ({ ...p, b: url }));
                    }
                  }}
                />
                <div className="grid grid-cols-2 gap-2">
                  {previews.a && (
                    <img
                      src={previews.a}
                      alt="Preview A"
                      className="w-full rounded-lg border border-slate-800"
                    />
                  )}
                  {previews.b && (
                    <img
                      src={previews.b}
                      alt="Preview B"
                      className="w-full rounded-lg border border-slate-800"
                    />
                  )}
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <Label>Base quality</Label>
                    <Input
                      type="number"
                      min={1}
                      max={100}
                      value={bakeInputs.baseQ}
                      onChange={(e) =>
                        setBakeInputs((s) => ({ ...s, baseQ: Number(e.target.value || 95) }))
                      }
                    />
                  </div>
                  <div>
                    <Label>Gainmap quality</Label>
                    <Input
                      type="number"
                      min={1}
                      max={100}
                      value={bakeInputs.gainmapQ}
                      onChange={(e) =>
                        setBakeInputs((s) => ({ ...s, gainmapQ: Number(e.target.value || 95) }))
                      }
                    />
                  </div>
                  <div>
                    <Label>Scale</Label>
                    <Input
                      type="number"
                      min={1}
                      value={bakeInputs.scale}
                      onChange={(e) =>
                        setBakeInputs((s) => ({ ...s, scale: Number(e.target.value || 1) }))
                      }
                    />
                  </div>
                  <div>
                    <Label>Target peak (nits)</Label>
                    <Input
                      type="number"
                      placeholder="auto"
                      value={bakeInputs.targetPeak}
                      onChange={(e) =>
                        setBakeInputs((s) => ({ ...s, targetPeak: e.target.value }))
                      }
                    />
                  </div>
                  <div className="flex items-end gap-2">
                    <input
                      id="mc"
                      type="checkbox"
                      checked={bakeInputs.multichannel}
                      onChange={(e) =>
                        setBakeInputs((s) => ({ ...s, multichannel: e.target.checked }))
                      }
                      className="h-4 w-4 rounded border-slate-600 bg-slate-900"
                    />
                    <Label htmlFor="mc">Use multi-channel gain map</Label>
                  </div>
                </div>
              </div>

              <div className="flex flex-col gap-3">
                <Button onClick={handleBake} className="w-full">
                  Run bake
                </Button>
                <Textarea
                  readOnly
                  value={log.join("\n")}
                  className="h-40 text-xs font-mono"
                  placeholder="Logs will appear here…"
                />
                {outputUrl && (
                  <div className="space-y-2 rounded-lg border border-slate-800 bg-slate-900/50 p-3">
                    <div className="flex items-center justify-between">
                      <p className="text-sm text-slate-300">{outputName}</p>
                      <a
                        href={outputUrl}
                        download={outputName || "ultrahdr_bake_out.jpg"}
                        className="text-primary underline"
                      >
                        Download
                      </a>
                    </div>
                    <img
                      src={outputUrl}
                      alt="UltraHDR output"
                      className="w-full rounded-lg border border-slate-800"
                    />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="motion">
          <Card>
            <CardHeader>
              <CardTitle>Motion Photo (JPEG + MP4)</CardTitle>
              <CardDescription>Embed a short MP4 clip into a Motion Photo container.</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              <div className="space-y-3">
                <Label className="flex items-center gap-2">
                  <Upload className="h-4 w-4" /> Still photo (JPEG)
                </Label>
                <Input
                  type="file"
                  accept="image/jpeg"
                  onChange={(e) =>
                    setMotionInputs((s) => ({ ...s, photo: e.target.files?.[0] || null }))
                  }
                />
                <Label className="flex items-center gap-2">
                  <Upload className="h-4 w-4" /> Motion clip (MP4)
                </Label>
                <Input
                  type="file"
                  accept="video/mp4"
                  onChange={(e) =>
                    setMotionInputs((s) => ({ ...s, video: e.target.files?.[0] || null }))
                  }
                />
                <div>
                  <Label>Timestamp (µs)</Label>
                  <Input
                    type="number"
                    placeholder="0"
                    value={motionInputs.timestampUs}
                    onChange={(e) =>
                      setMotionInputs((s) => ({ ...s, timestampUs: e.target.value }))
                    }
                  />
                </div>
              </div>

              <div className="flex flex-col gap-3">
                <Button onClick={handleMotion} className="w-full">
                  Build Motion Photo
                </Button>
                <Textarea
                  readOnly
                  value={log.join("\n")}
                  className="h-40 text-xs font-mono"
                  placeholder="Logs will appear here…"
                />
                {outputUrl && (
                  <div className="space-y-2 rounded-lg border border-slate-800 bg-slate-900/50 p-3">
                    <div className="flex items-center justify-between">
                      <p className="text-sm text-slate-300">{outputName}</p>
                      <a
                        href={outputUrl}
                        download={outputName || "motionphoto.jpg"}
                        className="text-primary underline"
                      >
                        Download
                      </a>
                    </div>
                    <img
                      src={outputUrl}
                      alt="Motion Photo"
                      className="w-full rounded-lg border border-slate-800"
                    />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
