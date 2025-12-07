import React from "react";

export type Lang = "en" | "zh-CN" | "zh-TW" | "ja" | "ko";

type TranslationDict = Record<Lang, Record<string, string>>;

const translations: TranslationDict = {
  en: {
    headerTitle: "UltraHDR Studio",
    headerSubtitle:
      "Encode UltraHDR JPEGs or motion photos directly in your browser via WASI.",
    repoLink: "Repository",
    statusLabel: "Status",
    statusUsesShim: "Uses @bjorn3/browser_wasi_shim",
    tabBake: "UltraHDR Bake",
    tabMotion: "Motion Photo",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription:
      "Auto-detects HDR/SDR ordering; tweak quality knobs if needed.",
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
    tooltipInputA:
      "Upload HDR or SDR JPEG; ordering is auto-detected with Input B.",
    tooltipInputB:
      "Upload the second JPEG (HDR/SDR). Ordering is auto-detected.",
    inputPair: "Input JPEGs (choose two)",
    tooltipInputPair:
      "Upload the two HDR/SDR JPEGs together; ordering is auto-detected.",
    tooltipBaseQuality: "JPEG quality (1-100) for the base SDR layer.",
    tooltipGainmapQuality: "JPEG quality (1-100) for the gain map.",
    tooltipScale:
      "Gain map resolution scale (integer). 1 = same size; 2 = half res; 4 = quarter res.",
    tooltipTargetPeak:
      "Optional target peak brightness in nits. Leave blank for auto.",
    tooltipMultichannel:
      "Encode gain map with separate RGB channels instead of luma-only.",
    runBake: "Run bake",
    logsPlaceholder: "Logs will appear here…",
    outputName: "Output filename",
    download: "Download",
    outputAlt: "UltraHDR output",
    motionTitle: "Motion Photo (JPEG + MP4)",
    motionDescription: "Embed a short MP4 clip into a Motion Photo container.",
    motionPhoto: "Still photo (JPEG)",
    motionVideo: "Motion clip (MP4)",
    timestamp: "Timestamp (µs)",
    timestampPlaceholder: "0",
    tooltipMotionPhoto: "Upload the JPEG frame for the Motion Photo.",
    tooltipMotionVideo: "Upload the short MP4 clip to embed.",
    motionPair: "Photo + video (choose two)",
    tooltipMotionPair:
      "Upload the JPEG photo and MP4/MOV together; types are auto-detected.",
    tooltipTimestamp:
      "Optional microsecond offset where motion should start (default 0).",
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
    headerTitle: "UltraHDR Studio",
    headerSubtitle: "通过 WASI 在浏览器内直接编码 UltraHDR JPEG 或动图。",
    repoLink: "项目主页",
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
    tooltipInputA: "上传 HDR 或 SDR JPEG；与 B 配对后自动判断顺序。",
    tooltipInputB: "上传第二张 HDR/SDR JPEG，顺序自动判断。",
    inputPair: "输入 JPEG（选择两张）",
    tooltipInputPair: "同时上传两张 HDR/SDR JPEG，顺序自动判断。",
    tooltipBaseQuality: "基础 SDR 层的 JPEG 质量 (1-100)。",
    tooltipGainmapQuality: "增益图的 JPEG 质量 (1-100)。",
    tooltipScale:
      "增益图分辨率缩放（整数）。1 表示与基底相同，2 表示长宽各减半，4 表示再减半。",
    tooltipTargetPeak: "可选的目标峰值亮度（尼特），留空自动。",
    tooltipMultichannel: "将增益图编码为独立 RGB 通道，而非单通道亮度。",
    runBake: "开始合成",
    logsPlaceholder: "日志会显示在这里…",
    outputName: "输出文件名",
    download: "下载",
    outputAlt: "UltraHDR 输出",
    motionTitle: "动态照片（JPEG + MP4）",
    motionDescription: "将短 MP4 嵌入 Motion Photo 容器。",
    motionPhoto: "静态照片（JPEG）",
    motionVideo: "动态片段（MP4）",
    timestamp: "时间戳（微秒）",
    timestampPlaceholder: "0",
    tooltipMotionPhoto: "上传动态照片中的静态 JPEG 帧。",
    tooltipMotionVideo: "上传要内嵌的短 MP4 片段。",
    motionPair: "照片 + 视频（选择两个）",
    tooltipMotionPair: "同时上传 JPEG 照片和 MP4/MOV，自动识别类型。",
    tooltipTimestamp: "可选：运动开始的微秒偏移（默认 0）。",
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
    headerTitle: "UltraHDR Studio",
    headerSubtitle: "透過 WASI 在瀏覽器中直接編碼 UltraHDR JPEG 或動態照片。",
    repoLink: "專案首頁",
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
    tooltipInputA: "上傳 HDR 或 SDR JPEG；與 B 搭配後自動判斷順序。",
    tooltipInputB: "上傳第二張 HDR/SDR JPEG，順序自動判斷。",
    inputPair: "輸入 JPEG（選兩張）",
    tooltipInputPair: "同時上傳兩張 HDR/SDR JPEG，順序自動判斷。",
    tooltipBaseQuality: "基礎 SDR 層的 JPEG 品質 (1-100)。",
    tooltipGainmapQuality: "增益圖的 JPEG 品質 (1-100)。",
    tooltipScale:
      "增益圖解析度縮放（整數）。1 表示與基底相同，2 表示長寬各減半，4 再減半。",
    tooltipTargetPeak: "可選目標峰值亮度（尼特），空白為自動。",
    tooltipMultichannel: "將增益圖編碼為獨立 RGB 通道而非單通道亮度。",
    runBake: "開始合成",
    logsPlaceholder: "日誌會顯示在這裡…",
    outputName: "輸出檔名",
    download: "下載",
    outputAlt: "UltraHDR 輸出",
    motionTitle: "動態照片（JPEG + MP4）",
    motionDescription: "將短 MP4 內嵌到 Motion Photo 容器。",
    motionPhoto: "靜態照片（JPEG）",
    motionVideo: "動態片段（MP4）",
    timestamp: "時間戳（微秒）",
    timestampPlaceholder: "0",
    tooltipMotionPhoto: "上傳動態照片的靜態 JPEG 影像。",
    tooltipMotionVideo: "上傳要嵌入的短 MP4 影片。",
    motionPair: "照片 + 影片（選兩個）",
    tooltipMotionPair: "同時上傳 JPEG 照片和 MP4/MOV，會自動辨識型別。",
    tooltipTimestamp: "可選：動態開始的微秒位移（預設 0）。",
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
    headerTitle: "UltraHDR Studio",
    headerSubtitle:
      "WASI を使ってブラウザ上で UltraHDR JPEG やモーションフォトをエンコードします。",
    repoLink: "リポジトリ",
    statusLabel: "ステータス",
    statusUsesShim: "@bjorn3/browser_wasi_shim を使用",
    tabBake: "UltraHDR ベイク",
    tabMotion: "モーションフォト",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription:
      "HDR/SDR の順序を自動判定。必要に応じて品質を調整できます。",
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
    tooltipInputA:
      "HDR または SDR の JPEG をアップロード。B と組み合わせて順序を自動判定。",
    tooltipInputB: "2 枚目の HDR/SDR JPEG をアップロード。順序は自動判定。",
    inputPair: "入力 JPEG（2 枚）",
    tooltipInputPair:
      "HDR/SDR の 2 枚をまとめてアップロード。順序は自動判定します。",
    tooltipBaseQuality: "ベース SDR レイヤーの JPEG 品質 (1-100)。",
    tooltipGainmapQuality: "ゲインマップの JPEG 品質 (1-100)。",
    tooltipScale:
      "ゲインマップの解像度スケール（整数）。1 で同じ、2 で縦横半分、4 でさらに半分。",
    tooltipTargetPeak: "任意の目標ピーク輝度 (nit)。空欄で自動。",
    tooltipMultichannel:
      "ゲインマップを輝度のみではなく RGB で個別に符号化します。",
    runBake: "ベイクを実行",
    logsPlaceholder: "ログはここに表示されます…",
    outputName: "出力ファイル名",
    download: "ダウンロード",
    outputAlt: "UltraHDR 出力",
    motionTitle: "モーションフォト（JPEG + MP4）",
    motionDescription: "短い MP4 を Motion Photo コンテナに埋め込みます。",
    motionPhoto: "静止画（JPEG）",
    motionVideo: "動画クリップ（MP4）",
    timestamp: "タイムスタンプ（µs）",
    timestampPlaceholder: "0",
    tooltipMotionPhoto: "モーションフォト用の静止 JPEG をアップロード。",
    tooltipMotionVideo: "埋め込む短い MP4 クリップをアップロード。",
    motionPair: "写真 + 動画（2 ファイル）",
    tooltipMotionPair:
      "JPEG 写真と MP4/MOV をまとめてアップロード。タイプは自動判別します。",
    tooltipTimestamp:
      "任意: モーション開始のマイクロ秒オフセット（デフォルト 0）。",
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
    headerTitle: "UltraHDR Studio",
    headerSubtitle:
      "WASI를 통해 브라우저에서 직접 UltraHDR JPEG 또는 모션 포토를 인코딩합니다.",
    repoLink: "저장소",
    statusLabel: "상태",
    statusUsesShim: "@bjorn3/browser_wasi_shim 사용",
    tabBake: "UltraHDR 베이크",
    tabMotion: "모션 포토",
    bakeTitle: "HDR + SDR ➜ UltraHDR",
    bakeDescription:
      "HDR/SDR 순서를 자동 감지하며, 필요하면 품질을 조정하세요.",
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
    tooltipInputA: "HDR 또는 SDR JPEG 업로드. B와 함께 순서를 자동 감지합니다.",
    tooltipInputB: "두 번째 HDR/SDR JPEG 업로드. 순서 자동 감지.",
    inputPair: "입력 JPEG (2개 선택)",
    tooltipInputPair:
      "HDR/SDR JPEG 두 장을 한 번에 업로드하세요. 순서는 자동 판단합니다.",
    tooltipBaseQuality: "베이스 SDR 레이어의 JPEG 품질 (1-100).",
    tooltipGainmapQuality: "게인맵 JPEG 품질 (1-100).",
    tooltipScale:
      "게인맵 해상도 배율(정수). 1은 동일, 2는 가로세로 절반, 4는 그보다 더 절반.",
    tooltipTargetPeak: "선택: 목표 피크 휘도(니트). 비워두면 자동.",
    tooltipMultichannel: "게인맵을 밝기 대신 RGB 각 채널로 인코딩합니다.",
    runBake: "베이크 실행",
    logsPlaceholder: "로그가 여기 표시됩니다…",
    outputName: "출력 파일 이름",
    download: "다운로드",
    outputAlt: "UltraHDR 출력",
    motionTitle: "모션 포토 (JPEG + MP4)",
    motionDescription: "짧은 MP4를 Motion Photo 컨테이너에 삽입합니다.",
    motionPhoto: "정지 사진 (JPEG)",
    motionVideo: "모션 클립 (MP4)",
    timestamp: "타임스탬프 (µs)",
    timestampPlaceholder: "0",
    tooltipMotionPhoto: "모션 포토의 정지 JPEG 프레임을 업로드하세요.",
    tooltipMotionVideo: "삽입할 짧은 MP4 클립을 업로드하세요.",
    motionPair: "사진 + 영상 (2개 선택)",
    tooltipMotionPair:
      "JPEG 사진과 MP4/MOV를 한 번에 업로드하면 유형을 자동 인식합니다.",
    tooltipTimestamp: "선택: 모션 시작 마이크로초 오프셋(기본 0).",
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

export function statusKeyFromMessage(
  message: string,
): TranslationKey | undefined {
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

const I18nContext = React.createContext<I18nContextValue | undefined>(
  undefined,
);

function interpolate(
  template: string,
  params?: Record<string, string | number>,
) {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (_, name) =>
    params[name] !== undefined ? String(params[name]) : `{${name}}`,
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
    [lang],
  );

  const translateStatus = React.useCallback(
    (message: string) => {
      const key = statusKeyFromMessage(message);
      if (key) return t(key);
      return message;
    },
    [t],
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
