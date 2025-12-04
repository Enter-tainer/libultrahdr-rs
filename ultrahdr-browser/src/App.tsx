import React from "react";
import { Upload, Wand2, Film } from "lucide-react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./components/ui/tabs";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui/card";
import { Label } from "./components/ui/label";
import { Input } from "./components/ui/input";
import { Button } from "./components/ui/button";
import { Textarea } from "./components/ui/textarea";
import { Select } from "./components/ui/select";
import {
  useI18n,
  supportedLanguages,
  statusKeyFromMessage,
  type Lang,
  type TranslationKey,
} from "./lib/i18n";
import type { WorkerStatus } from "./types/worker";

type Mode = "bake" | "motion";
type StatusEntry = { key?: TranslationKey; text?: string; params?: Record<string, string | number> };

const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });

export default function App() {
  const { t, translateStatus, lang, setLang } = useI18n();
  const [mode, setMode] = React.useState<Mode>("bake");
  const [status, setStatus] = React.useState<StatusEntry>({ key: "statusIdle" });
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
  const resolvedStatus = React.useMemo(
    () => (status.key ? t(status.key, status.params) : status.text || ""),
    [status, t]
  );

  React.useEffect(() => {
    worker.onmessage = (event: MessageEvent<WorkerStatus>) => {
      const { type, payload } = event.data;
      if (type === "status") {
        const key = statusKeyFromMessage(payload);
        if (key) {
          const msg = t(key);
          setStatus({ key });
          setLog((prev) => [...prev, msg]);
        } else {
          const translated = translateStatus(payload);
          setStatus({ text: translated });
          setLog((prev) => [...prev, translated]);
        }
      } else if (type === "stdout") {
        setLog((prev) => [...prev, payload]);
      } else if (type === "stderr") {
        setLog((prev) => [...prev, `${t("logErrorPrefix")} ${payload}`]);
      } else if (type === "done") {
        const blob = new Blob([payload.buffer], { type: "image/jpeg" });
        const url = URL.createObjectURL(blob);
        setOutputUrl(url);
        setOutputName(payload.fileName);
        setStatus({ key: "statusDone" });
        setLog((prev) => [...prev, t("wroteFile", { file: payload.fileName })]);
      } else if (type === "error") {
        setStatus({ key: "statusError" });
        setLog((prev) => [...prev, `${t("logErrorPrefix")} ${payload}`]);
      }
    };
  }, [t, translateStatus]);

  const handleBake = async () => {
    if (!bakeInputs.hdr || !bakeInputs.sdr) {
      setStatus({ key: "statusNeedTwoJpegs" });
      setLog((prev) => [...prev, t("statusNeedTwoJpegs")]);
      return;
    }
    setOutputUrl(null);
    setLog([]);
    setStatus({ key: "statusPreparing" });
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
      setStatus({ key: "statusNeedPhotoVideo" });
      setLog((prev) => [...prev, t("statusNeedPhotoVideo")]);
      return;
    }
    setOutputUrl(null);
    setLog([]);
    setStatus({ key: "statusPreparing" });
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
      <header className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
        <div>
          <p className="text-sm uppercase tracking-[0.3em] text-slate-400">UltraHDR</p>
          <h1 className="text-3xl font-semibold text-white">{t("headerTitle")}</h1>
          <p className="text-slate-400">{t("headerSubtitle")}</p>
        </div>
        <div className="flex flex-col gap-2 md:items-end">
          <a
            href="https://github.com/Enter-tainer/libultrahdr-rs"
            target="_blank"
            rel="noreferrer"
            className="inline-flex w-fit items-center gap-2 rounded-lg border border-slate-800 bg-slate-900/70 px-3 py-2 text-sm text-slate-100 hover:border-slate-700 hover:bg-slate-800"
            title="Open repository in GitHub"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 24 24"
              fill="currentColor"
              className="h-4 w-4"
              aria-hidden
            >
              <path d="M12 .5a12 12 0 0 0-3.79 23.4c.6.11.82-.26.82-.58v-2.1c-3.34.73-4.04-1.61-4.04-1.61-.55-1.4-1.35-1.77-1.35-1.77-1.1-.76.08-.74.08-.74 1.22.09 1.86 1.25 1.86 1.25 1.08 1.85 2.83 1.32 3.52 1 .11-.78.42-1.32.76-1.62-2.66-.31-5.46-1.33-5.46-5.9 0-1.3.47-2.36 1.24-3.19-.12-.31-.54-1.55.12-3.22 0 0 1-.32 3.3 1.22a11.5 11.5 0 0 1 6 0c2.3-1.54 3.29-1.22 3.29-1.22.66 1.67.24 2.9.12 3.22.78.83 1.23 1.9 1.23 3.2 0 4.59-2.8 5.58-5.47 5.88.43.36.81 1.08.81 2.17v3.22c0 .32.21.7.82.58A12 12 0 0 0 12 .5Z" />
            </svg>
            {t("repoLink")}
          </a>
          <div className="flex items-center gap-2">
            <Label className="text-slate-300">{t("languageLabel")}</Label>
            <Select
              aria-label={t("languageLabel")}
              value={lang}
              onChange={(e) => setLang(e.target.value as Lang)}
              className="w-40"
            >
              {supportedLanguages.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </Select>
          </div>
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 px-4 py-3 text-sm text-slate-300">
            <p>
              {t("statusLabel")}: {resolvedStatus}
            </p>
            <p className="text-xs text-slate-500">{t("statusUsesShim")}</p>
          </div>
        </div>
      </header>

      <Tabs defaultValue="bake" className="w-full">
        <TabsList className="bg-slate-900">
          <TabsTrigger value="bake" onClick={() => setMode("bake")}>
            <Wand2 className="mr-2 h-4 w-4" />
            {t("tabBake")}
          </TabsTrigger>
          <TabsTrigger value="motion" onClick={() => setMode("motion")}>
            <Film className="mr-2 h-4 w-4" />
            {t("tabMotion")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="bake">
          <Card>
            <CardHeader>
              <CardTitle>{t("bakeTitle")}</CardTitle>
              <CardDescription>{t("bakeDescription")}</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              <div className="space-y-3">
                <Label className="flex items-center gap-2" title={t("tooltipInputA")}>
                  <Upload className="h-4 w-4" /> {t("inputA")}
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
                <Label className="flex items-center gap-2" title={t("tooltipInputB")}>
                  <Upload className="h-4 w-4" /> {t("inputB")}
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
                      alt={t("previewA")}
                      className="w-full rounded-lg border border-slate-800"
                    />
                  )}
                  {previews.b && (
                    <img
                      src={previews.b}
                      alt={t("previewB")}
                      className="w-full rounded-lg border border-slate-800"
                    />
                  )}
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <Label title={t("tooltipBaseQuality")}>{t("baseQuality")}</Label>
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
                    <Label title={t("tooltipGainmapQuality")}>{t("gainmapQuality")}</Label>
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
                    <Label title={t("tooltipScale")}>{t("scale")}</Label>
                    <Input
                      type="number"
                      min={1}
                      step={1}
                      value={bakeInputs.scale}
                      onChange={(e) =>
                        setBakeInputs((s) => ({
                          ...s,
                          scale: Number(e.target.value || 1),
                        }))
                      }
                    />
                  </div>
                  <div>
                    <Label title={t("tooltipTargetPeak")}>{t("targetPeak")}</Label>
                    <Input
                      type="number"
                      placeholder={t("targetPeakPlaceholder")}
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
                    <Label htmlFor="mc" title={t("tooltipMultichannel")}>
                      {t("multichannel")}
                    </Label>
                  </div>
                </div>
              </div>

              <div className="flex flex-col gap-3">
                <Button onClick={handleBake} className="w-full">
                  {t("runBake")}
                </Button>
                <Textarea
                  readOnly
                  value={log.join("\n")}
                  className="h-40 text-xs font-mono"
                  placeholder={t("logsPlaceholder")}
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
                        {t("download")}
                      </a>
                    </div>
                    <img
                      src={outputUrl}
                      alt={t("outputAlt")}
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
              <CardTitle>{t("motionTitle")}</CardTitle>
              <CardDescription>{t("motionDescription")}</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              <div className="space-y-3">
                <Label className="flex items-center gap-2" title={t("tooltipMotionPhoto")}>
                  <Upload className="h-4 w-4" /> {t("motionPhoto")}
                </Label>
                <Input
                  type="file"
                  accept="image/jpeg"
                  onChange={(e) =>
                    setMotionInputs((s) => ({ ...s, photo: e.target.files?.[0] || null }))
                  }
                />
                <Label className="flex items-center gap-2" title={t("tooltipMotionVideo")}>
                  <Upload className="h-4 w-4" /> {t("motionVideo")}
                </Label>
                <Input
                  type="file"
                  accept="video/mp4"
                  onChange={(e) =>
                    setMotionInputs((s) => ({ ...s, video: e.target.files?.[0] || null }))
                  }
                />
                <div>
                  <Label title={t("tooltipTimestamp")}>{t("timestamp")}</Label>
                  <Input
                    type="number"
                    placeholder={t("timestampPlaceholder")}
                    value={motionInputs.timestampUs}
                    onChange={(e) =>
                      setMotionInputs((s) => ({ ...s, timestampUs: e.target.value }))
                    }
                  />
                </div>
              </div>

              <div className="flex flex-col gap-3">
                <Button onClick={handleMotion} className="w-full">
                  {t("buildMotion")}
                </Button>
                <Textarea
                  readOnly
                  value={log.join("\n")}
                  className="h-40 text-xs font-mono"
                  placeholder={t("logsPlaceholder")}
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
                        {t("download")}
                      </a>
                    </div>
                    <img
                      src={outputUrl}
                      alt={t("motionAlt")}
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
