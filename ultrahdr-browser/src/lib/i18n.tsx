import React from "react";

export type Lang = "en" | "zh-CN" | "zh-TW" | "ja" | "ko";

type TranslationDict = Record<Lang, Record<string, string>>;

const translations: TranslationDict = {
  en: {
    headerTitle: "WASI Browser Studio",
    headerSubtitle: "Encode UltraHDR JPEGs or motion photos directly in your browser via WASI.",
    statusLabel: "Status",
    statusUsesShim: "Uses @bjorn3/browser_wasi_shim",
    tabBake: "UltraHDR Bake",
    tabMotion: "Motion Photo",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription: "Auto-detects HDR/SDR ordering; tweak quality knobs if needed.",
    inputA: "Input JPEG A",
    inputB: "Input JPEG B",
    previewA: "Preview A",
    previewB: "Preview B",
    baseQuality: "Base quality",
    gainmapQuality: "Gainmap quality",
    scale: "Scale",
    targetPeak: "Target peak (nits)",
    targetPeakPlaceholder: "auto",
    multichannel: "Use multi-channel gain map",
    runBake: "Run bake",
    logsPlaceholder: "Logs will appear here…",
    download: "Download",
    outputAlt: "UltraHDR output",
    motionTitle: "Motion Photo (JPEG + MP4)",
    motionDescription: "Embed a short MP4 clip into a Motion Photo container.",
    motionPhoto: "Still photo (JPEG)",
    motionVideo: "Motion clip (MP4)",
    timestamp: "Timestamp (µs)",
    timestampPlaceholder: "0",
    buildMotion: "Build Motion Photo",
    motionAlt: "Motion Photo",
    languageLabel: "Language",
    statusIdle: "Idle",
    statusPreparing: "Preparing…",
    statusNeedTwoJpegs: "Please choose two JPEG files",
    statusNeedPhotoVideo: "Please choose both photo and video",
    statusDone: "Done",
    statusError: "Error",
    statusPreparingFs: "Preparing WASI FS…",
    statusFetchingWasm: "Fetching wasm…",
    statusRunning: "Running ultrahdr-bake…",
    statusOutputMissing: "Output file missing",
    wroteFile: "Wrote {file}",
    logErrorPrefix: "[err]",
  },
  "zh-CN": {
    headerTitle: "WASI 浏览器工作室",
    headerSubtitle: "通过 WASI 在浏览器内直接编码 UltraHDR JPEG 或动图。",
    statusLabel: "状态",
    statusUsesShim: "基于 @bjorn3/browser_wasi_shim",
    tabBake: "UltraHDR 合成",
    tabMotion: "动态照片",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription: "自动识别 HDR/SDR 顺序；需要时可调节质量。",
    inputA: "输入 JPEG A",
    inputB: "输入 JPEG B",
    previewA: "预览 A",
    previewB: "预览 B",
    baseQuality: "基础质量",
    gainmapQuality: "增益图质量",
    scale: "缩放",
    targetPeak: "目标峰值（尼特）",
    targetPeakPlaceholder: "自动",
    multichannel: "使用多通道增益图",
    runBake: "开始合成",
    logsPlaceholder: "日志会显示在这里…",
    download: "下载",
    outputAlt: "UltraHDR 输出",
    motionTitle: "动态照片（JPEG + MP4）",
    motionDescription: "将短 MP4 嵌入 Motion Photo 容器。",
    motionPhoto: "静态照片（JPEG）",
    motionVideo: "动态片段（MP4）",
    timestamp: "时间戳（微秒）",
    timestampPlaceholder: "0",
    buildMotion: "生成动态照片",
    motionAlt: "动态照片",
    languageLabel: "语言",
    statusIdle: "空闲",
    statusPreparing: "准备中…",
    statusNeedTwoJpegs: "请选择两张 JPEG 文件",
    statusNeedPhotoVideo: "请选择照片和视频",
    statusDone: "完成",
    statusError: "错误",
    statusPreparingFs: "准备 WASI 文件系统…",
    statusFetchingWasm: "获取 wasm…",
    statusRunning: "运行 ultrahdr-bake…",
    statusOutputMissing: "输出文件缺失",
    wroteFile: "已写入 {file}",
    logErrorPrefix: "[错误]",
  },
  "zh-TW": {
    headerTitle: "WASI 瀏覽器工作室",
    headerSubtitle: "透過 WASI 在瀏覽器中直接編碼 UltraHDR JPEG 或動態照片。",
    statusLabel: "狀態",
    statusUsesShim: "使用 @bjorn3/browser_wasi_shim",
    tabBake: "UltraHDR 合成",
    tabMotion: "動態照片",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription: "自動判斷 HDR/SDR 順序；需要時可調整品質。",
    inputA: "輸入 JPEG A",
    inputB: "輸入 JPEG B",
    previewA: "預覽 A",
    previewB: "預覽 B",
    baseQuality: "基礎品質",
    gainmapQuality: "增益圖品質",
    scale: "縮放",
    targetPeak: "目標峰值（尼特）",
    targetPeakPlaceholder: "自動",
    multichannel: "使用多通道增益圖",
    runBake: "開始合成",
    logsPlaceholder: "日誌會顯示在這裡…",
    download: "下載",
    outputAlt: "UltraHDR 輸出",
    motionTitle: "動態照片（JPEG + MP4）",
    motionDescription: "將短 MP4 內嵌到 Motion Photo 容器。",
    motionPhoto: "靜態照片（JPEG）",
    motionVideo: "動態片段（MP4）",
    timestamp: "時間戳（微秒）",
    timestampPlaceholder: "0",
    buildMotion: "產生動態照片",
    motionAlt: "動態照片",
    languageLabel: "語言",
    statusIdle: "閒置",
    statusPreparing: "準備中…",
    statusNeedTwoJpegs: "請選擇兩張 JPEG 檔",
    statusNeedPhotoVideo: "請選擇照片與影片",
    statusDone: "完成",
    statusError: "錯誤",
    statusPreparingFs: "準備 WASI 檔案系統…",
    statusFetchingWasm: "抓取 wasm…",
    statusRunning: "執行 ultrahdr-bake…",
    statusOutputMissing: "找不到輸出檔",
    wroteFile: "已寫入 {file}",
    logErrorPrefix: "[錯誤]",
  },
  ja: {
    headerTitle: "WASI ブラウザスタジオ",
    headerSubtitle: "WASI を使ってブラウザ上で UltraHDR JPEG やモーションフォトをエンコードします。",
    statusLabel: "ステータス",
    statusUsesShim: "@bjorn3/browser_wasi_shim を使用",
    tabBake: "UltraHDR ベイク",
    tabMotion: "モーションフォト",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription: "HDR/SDR の順序を自動判定。必要に応じて品質を調整できます。",
    inputA: "入力 JPEG A",
    inputB: "入力 JPEG B",
    previewA: "プレビュー A",
    previewB: "プレビュー B",
    baseQuality: "ベース品質",
    gainmapQuality: "ゲインマップ品質",
    scale: "スケール",
    targetPeak: "目標ピーク（ニット）",
    targetPeakPlaceholder: "自動",
    multichannel: "マルチチャネルのゲインマップを使用",
    runBake: "ベイクを実行",
    logsPlaceholder: "ログはここに表示されます…",
    download: "ダウンロード",
    outputAlt: "UltraHDR 出力",
    motionTitle: "モーションフォト（JPEG + MP4）",
    motionDescription: "短い MP4 を Motion Photo コンテナに埋め込みます。",
    motionPhoto: "静止画（JPEG）",
    motionVideo: "動画クリップ（MP4）",
    timestamp: "タイムスタンプ（µs）",
    timestampPlaceholder: "0",
    buildMotion: "モーションフォトを作成",
    motionAlt: "モーションフォト",
    languageLabel: "言語",
    statusIdle: "待機中",
    statusPreparing: "準備中…",
    statusNeedTwoJpegs: "2 枚の JPEG を選択してください",
    statusNeedPhotoVideo: "写真と動画を選択してください",
    statusDone: "完了",
    statusError: "エラー",
    statusPreparingFs: "WASI ファイルシステムを準備中…",
    statusFetchingWasm: "wasm を取得中…",
    statusRunning: "ultrahdr-bake 実行中…",
    statusOutputMissing: "出力ファイルが見つかりません",
    wroteFile: "{file} を書き出しました",
    logErrorPrefix: "[エラー]",
  },
  ko: {
    headerTitle: "WASI 브라우저 스튜디오",
    headerSubtitle: "WASI를 통해 브라우저에서 직접 UltraHDR JPEG 또는 모션 포토를 인코딩합니다.",
    statusLabel: "상태",
    statusUsesShim: "@bjorn3/browser_wasi_shim 사용",
    tabBake: "UltraHDR 베이크",
    tabMotion: "모션 포토",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription: "HDR/SDR 순서를 자동 감지하며, 필요하면 품질을 조정하세요.",
    inputA: "입력 JPEG A",
    inputB: "입력 JPEG B",
    previewA: "미리보기 A",
    previewB: "미리보기 B",
    baseQuality: "베이스 품질",
    gainmapQuality: "게인맵 품질",
    scale: "스케일",
    targetPeak: "목표 피크 (니트)",
    targetPeakPlaceholder: "자동",
    multichannel: "멀티 채널 게인맵 사용",
    runBake: "베이크 실행",
    logsPlaceholder: "로그가 여기 표시됩니다…",
    download: "다운로드",
    outputAlt: "UltraHDR 출력",
    motionTitle: "모션 포토 (JPEG + MP4)",
    motionDescription: "짧은 MP4를 Motion Photo 컨테이너에 삽입합니다.",
    motionPhoto: "정지 사진 (JPEG)",
    motionVideo: "모션 클립 (MP4)",
    timestamp: "타임스탬프 (µs)",
    timestampPlaceholder: "0",
    buildMotion: "모션 포토 생성",
    motionAlt: "모션 포토",
    languageLabel: "언어",
    statusIdle: "대기 중",
    statusPreparing: "준비 중…",
    statusNeedTwoJpegs: "JPEG 두 개를 선택하세요",
    statusNeedPhotoVideo: "사진과 영상을 선택하세요",
    statusDone: "완료",
    statusError: "오류",
    statusPreparingFs: "WASI 파일 시스템 준비 중…",
    statusFetchingWasm: "wasm 가져오는 중…",
    statusRunning: "ultrahdr-bake 실행 중…",
    statusOutputMissing: "출력 파일이 없습니다",
    wroteFile: "{file} 저장 완료",
    logErrorPrefix: "[오류]",
  },
};

export type TranslationKey = keyof typeof translations.en;

const statusKeyByMessage: Record<string, TranslationKey> = {
  "Preparing WASI FS…": "statusPreparingFs",
  "Fetching wasm…": "statusFetchingWasm",
  "Running ultrahdr-bake…": "statusRunning",
  "Output file missing": "statusOutputMissing",
};

export function statusKeyFromMessage(message: string): TranslationKey | undefined {
  return statusKeyByMessage[message];
}

const languageLabels: Record<Lang, string> = {
  en: "English",
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
  ja: "日本語",
  ko: "한국어",
};

export const supportedLanguages: { value: Lang; label: string }[] = [
  { value: "en", label: languageLabels.en },
  { value: "zh-CN", label: languageLabels["zh-CN"] },
  { value: "zh-TW", label: languageLabels["zh-TW"] },
  { value: "ja", label: languageLabels.ja },
  { value: "ko", label: languageLabels.ko },
];

type I18nContextValue = {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
  translateStatus: (message: string) => string;
};

const I18nContext = React.createContext<I18nContextValue | undefined>(undefined);

function interpolate(template: string, params?: Record<string, string | number>) {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (_, name) =>
    params[name] !== undefined ? String(params[name]) : `{${name}}`
  );
}

function matchLang(value?: string): Lang | undefined {
  if (!value) return undefined;
  const lower = value.toLowerCase();
  if (lower.startsWith("zh-cn") || lower.startsWith("zh-hans")) return "zh-CN";
  if (lower.startsWith("zh-tw") || lower.startsWith("zh-hant")) return "zh-TW";
  if (lower.startsWith("ja")) return "ja";
  if (lower.startsWith("ko")) return "ko";
  if (lower.startsWith("en")) return "en";
  return undefined;
}

function detectInitialLang(): Lang {
  if (typeof localStorage !== "undefined") {
    const cached = localStorage.getItem("ultrahdr-lang");
    const matched = matchLang(cached || undefined);
    if (matched) return matched;
  }
  if (typeof navigator !== "undefined") {
    const navLangs = navigator.languages || [navigator.language];
    for (const val of navLangs) {
      const matched = matchLang(val);
      if (matched) return matched;
    }
  }
  return "en";
}

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLangState] = React.useState<Lang>(() => detectInitialLang());

  const setLang = React.useCallback((value: Lang) => {
    setLangState(value);
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("ultrahdr-lang", value);
    }
  }, []);

  const t = React.useCallback(
    (key: TranslationKey, params?: Record<string, string | number>) => {
      const dict = translations[lang] || translations.en;
      const fallback = translations.en;
      const template = dict[key] ?? fallback[key] ?? key;
      return interpolate(template, params);
    },
    [lang]
  );

  const translateStatus = React.useCallback(
    (message: string) => {
      const key = statusKeyFromMessage(message);
      if (key) return t(key);
      return message;
    },
    [t]
  );

  return (
    <I18nContext.Provider value={{ lang, setLang, t, translateStatus }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  const ctx = React.useContext(I18nContext);
  if (!ctx) {
    throw new Error("useI18n must be used within I18nProvider");
  }
  return ctx;
}
