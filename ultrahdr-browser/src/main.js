const hdrInput = document.getElementById("hdr-file");
const sdrInput = document.getElementById("sdr-file");
const runBtn = document.getElementById("run-btn");
const statusEl = document.getElementById("status");
const resultEl = document.getElementById("result");
const dlLink = document.getElementById("download-link");
const previewImg = document.getElementById("preview-img");

const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });

function setStatus(msg) {
  statusEl.textContent = msg;
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

  const hdrBytes = await hdrFile.arrayBuffer();
  const sdrBytes = await sdrFile.arrayBuffer();

  setStatus("Handing off to worker…");
  worker.postMessage(
    {
      hdr: hdrBytes,
      sdr: sdrBytes,
    },
    [hdrBytes, sdrBytes]
  );
}

runBtn.addEventListener("click", () => {
  bake().catch((err) => {
    console.error(err);
    setStatus(`Error: ${err.message}`);
  });
});

worker.onmessage = (event) => {
  const { type, payload } = event.data;
  if (type === "status") {
    setStatus(payload);
  } else if (type === "done") {
    const blob = new Blob([payload], { type: "image/jpeg" });
    const url = URL.createObjectURL(blob);
    dlLink.href = url;
    previewImg.src = url;
    resultEl.classList.remove("hidden");
    setStatus("Done.");
  } else if (type === "error") {
    console.error(payload);
    setStatus(`Error: ${payload}`);
  }
};
