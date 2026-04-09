<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

type ModelFileInfo = {
  filename: string;
  path: string;
  sizeBytes: number;
};

type DownloadModelResult = {
  filename: string;
  path: string;
  sizeBytes: number;
};

type ProbeResult = {
  modelFilename: string;
  modelPath: string;
  prompt: string;
  output: string;
  usedChatTemplate: boolean;
  promptTokens: number;
  generatedTokens: number;
  loadMs: number;
  firstTokenMs: number | null;
  totalMs: number;
  devices: string[];
};

const prompt = ref(
  "The user says: Today the most draining thing was a meeting with coworkers.",
);
const modelUrl = ref("");
const modelFilename = ref("");
const models = ref<ModelFileInfo[]>([]);
const downloadInfo = ref<DownloadModelResult | null>(null);
const probeResult = ref<ProbeResult | null>(null);
const errorMessage = ref("");
const downloading = ref(false);
const running = ref(false);

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) {
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

async function refreshModels() {
  models.value = await invoke<ModelFileInfo[]>("list_models");
  if (!modelFilename.value && models.value.length > 0) {
    modelFilename.value = models.value[0].filename;
  }
}

async function handleDownload() {
  errorMessage.value = "";
  probeResult.value = null;
  downloading.value = true;

  try {
    const result = await invoke<DownloadModelResult>("download_model", {
      url: modelUrl.value,
      filename: modelFilename.value || null,
    });
    downloadInfo.value = result;
    modelFilename.value = result.filename;
    await refreshModels();
  } catch (error) {
    errorMessage.value = String(error);
  } finally {
    downloading.value = false;
  }
}

async function handleRunProbe() {
  errorMessage.value = "";
  probeResult.value = null;
  running.value = true;

  try {
    probeResult.value = await invoke<ProbeResult>("run_probe", {
      req: {
        modelFilename: modelFilename.value,
        prompt: prompt.value,
        maxTokens: 16,
      },
    });
  } catch (error) {
    errorMessage.value = String(error);
  } finally {
    running.value = false;
  }
}

onMounted(async () => {
  try {
    await refreshModels();
  } catch (error) {
    errorMessage.value = String(error);
  }
});
</script>

<template>
  <main class="min-h-screen bg-base-200 px-4 py-6 text-base-content">
    <div class="mx-auto flex max-w-5xl flex-col gap-6">
      <section class="hero rounded-box bg-base-100 shadow-xl">
        <div class="hero-content text-center">
          <div class="max-w-2xl">
            <div class="badge badge-accent badge-outline mb-4">
              local llm apk probe
            </div>
            <h1 class="text-4xl font-black">Petal Journal Probe</h1>
            <p class="mt-3 text-sm opacity-70">
              Download one GGUF model into app storage, run one fixed journaling
              prompt, then watch whether your phone can survive it.
            </p>
          </div>
        </div>
      </section>

      <section class="grid gap-6 lg:grid-cols-[1.2fr_0.8fr]">
        <div class="card bg-base-100 shadow-xl">
          <div class="card-body gap-4">
            <div>
              <h2 class="card-title">1. Download or Reuse a Model</h2>
              <p class="text-sm opacity-70">
                Paste a direct GGUF file URL or pick one of the files already in
                app storage.
              </p>
            </div>

            <label class="form-control gap-2">
              <span class="label-text font-semibold">Model URL</span>
              <input
                v-model="modelUrl"
                class="input input-bordered w-full"
                placeholder="https://.../model.gguf"
              />
            </label>

            <label class="form-control gap-2">
              <span class="label-text font-semibold">Model Filename</span>
              <input
                v-model="modelFilename"
                class="input input-bordered w-full"
                placeholder="Qwen2.5-0.5B-Instruct-Q4_K_M.gguf"
              />
            </label>

            <div class="flex flex-wrap gap-3">
              <button
                class="btn btn-primary"
                :class="{ 'btn-disabled': downloading }"
                @click="handleDownload"
              >
                <span
                  v-if="downloading"
                  class="loading loading-spinner loading-sm"
                ></span>
                Download model
              </button>
              <button
                class="btn btn-secondary"
                :class="{ 'btn-disabled': running || !modelFilename }"
                @click="handleRunProbe"
              >
                <span
                  v-if="running"
                  class="loading loading-spinner loading-sm"
                ></span>
                Run local probe
              </button>
            </div>

            <label class="form-control gap-2">
              <span class="label-text font-semibold">Prompt</span>
              <textarea
                v-model="prompt"
                class="textarea textarea-bordered min-h-32 w-full"
              ></textarea>
            </label>
          </div>
        </div>

        <div class="card bg-base-100 shadow-xl">
          <div class="card-body gap-4">
            <h2 class="card-title">2. What Exists Right Now</h2>
            <div class="rounded-box bg-base-200 p-3 text-xs opacity-80">
              Files are stored in the app's private data directory. This is the
              same location the Rust backend uses for the local probe.
            </div>

            <div class="flex flex-col gap-2">
              <div
                v-for="model in models"
                :key="model.path"
                class="rounded-box border border-base-300 bg-base-200 p-3"
              >
                <div class="font-semibold">{{ model.filename }}</div>
                <div class="text-xs opacity-70">{{ formatBytes(model.sizeBytes) }}</div>
              </div>
              <div
                v-if="models.length === 0"
                class="rounded-box border border-dashed border-base-300 p-4 text-sm opacity-60"
              >
                No model downloaded yet.
              </div>
            </div>
          </div>
        </div>
      </section>

      <section v-if="downloadInfo" class="card bg-base-100 shadow-xl">
        <div class="card-body">
          <h2 class="card-title">Last Download</h2>
          <div class="stats stats-vertical bg-base-200 lg:stats-horizontal">
            <div class="stat">
              <div class="stat-title">Filename</div>
              <div class="stat-value text-lg">{{ downloadInfo.filename }}</div>
            </div>
            <div class="stat">
              <div class="stat-title">Size</div>
              <div class="stat-value text-lg">
                {{ formatBytes(downloadInfo.sizeBytes) }}
              </div>
            </div>
          </div>
          <div class="mockup-code text-xs">
            <pre><code>{{ downloadInfo.path }}</code></pre>
          </div>
        </div>
      </section>

      <section v-if="probeResult" class="grid gap-6 lg:grid-cols-[1.1fr_0.9fr]">
        <div class="card bg-base-100 shadow-xl">
          <div class="card-body">
            <div class="flex items-center justify-between gap-3">
              <h2 class="card-title">Probe Output</h2>
              <div class="badge badge-success badge-outline">
                {{ probeResult.usedChatTemplate ? "chat template" : "fallback prompt" }}
              </div>
            </div>
            <div class="rounded-box bg-base-200 p-4 text-lg leading-8">
              {{ probeResult.output }}
            </div>
            <div class="mockup-code text-xs">
              <pre><code>{{ probeResult.modelPath }}</code></pre>
            </div>
          </div>
        </div>

        <div class="card bg-base-100 shadow-xl">
          <div class="card-body">
            <h2 class="card-title">Probe Metrics</h2>
            <div class="stats stats-vertical bg-base-200">
              <div class="stat">
                <div class="stat-title">Model Load</div>
                <div class="stat-value text-2xl">{{ probeResult.loadMs }} ms</div>
              </div>
              <div class="stat">
                <div class="stat-title">First Token</div>
                <div class="stat-value text-2xl">
                  {{ probeResult.firstTokenMs ?? "n/a" }}
                  <span v-if="probeResult.firstTokenMs !== null"> ms</span>
                </div>
              </div>
              <div class="stat">
                <div class="stat-title">Total Generation</div>
                <div class="stat-value text-2xl">{{ probeResult.totalMs }} ms</div>
              </div>
              <div class="stat">
                <div class="stat-title">Tokens</div>
                <div class="stat-value text-2xl">
                  {{ probeResult.promptTokens }} / {{ probeResult.generatedTokens }}
                </div>
                <div class="stat-desc">prompt / generated</div>
              </div>
            </div>

            <div class="divider">backend devices</div>

            <div class="flex flex-col gap-2">
              <div
                v-for="device in probeResult.devices"
                :key="device"
                class="rounded-box border border-base-300 bg-base-200 p-3 text-sm"
              >
                {{ device }}
              </div>
            </div>
          </div>
        </div>
      </section>

      <section v-if="errorMessage" class="alert alert-error shadow-lg">
        <span>{{ errorMessage }}</span>
      </section>
    </div>
  </main>
</template>
