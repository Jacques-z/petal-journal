use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

use llama_cpp_2::{
    context::params::LlamaContextParams,
    list_llama_ggml_backend_devices,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    openai::OpenAIChatTemplateParams,
    sampling::LlamaSampler,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager};

const DEFAULT_MODEL_FILENAME: &str = "model.gguf";
const DEFAULT_PROMPT: &str =
    "The user says: Today the most draining thing was a meeting with coworkers.";
const SYSTEM_PROMPT: &str =
    "You are a gentle journaling assistant. Ask exactly one short follow-up question. No advice. No analysis. No bullet points.";
static BACKEND_INIT_LOCK: Mutex<()> = Mutex::new(());
static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileInfo {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadModelResult {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProbeRequest {
    pub model_filename: String,
    pub prompt: Option<String>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub model_filename: String,
    pub model_path: String,
    pub prompt: String,
    pub output: String,
    pub used_chat_template: bool,
    pub prompt_tokens: usize,
    pub generated_tokens: usize,
    pub load_ms: u64,
    pub first_token_ms: Option<u64>,
    pub total_ms: u64,
    pub devices: Vec<String>,
}

#[tauri::command]
pub fn list_models(app: AppHandle) -> Result<Vec<ModelFileInfo>, String> {
    let models_dir = models_dir(&app)?;
    let mut items = Vec::new();

    for entry in fs::read_dir(&models_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = entry.metadata().map_err(|err| err.to_string())?;
        items.push(ModelFileInfo {
            filename: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            size_bytes: metadata.len(),
        });
    }

    items.sort_by(|left, right| left.filename.cmp(&right.filename));
    Ok(items)
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    url: String,
    filename: Option<String>,
) -> Result<DownloadModelResult, String> {
    let trimmed_url = url.trim();
    if trimmed_url.is_empty() {
        return Err("Model URL is required.".into());
    }

    let parsed_url = reqwest::Url::parse(trimmed_url).map_err(|err| err.to_string())?;
    let safe_name = filename
        .as_deref()
        .map(normalize_filename)
        .unwrap_or_else(|| {
            normalize_filename(
                parsed_url
                    .path_segments()
                    .and_then(|segments| segments.last())
                    .unwrap_or(DEFAULT_MODEL_FILENAME),
            )
        });

    let path = models_dir(&app)?.join(&safe_name);
    let path_for_worker = path.clone();
    let url_for_worker = parsed_url.to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|err| err.to_string())?;
        let mut response = client
            .get(url_for_worker)
            .send()
            .map_err(|err| err.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Download failed with HTTP {}.", response.status()));
        }

        let file = File::create(&path_for_worker).map_err(|err| err.to_string())?;
        let mut writer = BufWriter::new(file);
        std::io::copy(&mut response, &mut writer).map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;

        let size_bytes = fs::metadata(&path_for_worker)
            .map_err(|err| err.to_string())?
            .len();

        Ok(DownloadModelResult {
            filename: safe_name,
            path: path_for_worker.to_string_lossy().into_owned(),
            size_bytes,
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn run_probe(app: AppHandle, req: RunProbeRequest) -> Result<ProbeResult, String> {
    let model_filename = normalize_filename(&req.model_filename);
    let model_path = models_dir(&app)?.join(&model_filename);
    if !model_path.exists() {
        return Err(format!(
            "Model file not found in app storage: {}",
            model_filename
        ));
    }

    let prompt = req
        .prompt
        .unwrap_or_else(|| DEFAULT_PROMPT.to_string())
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("Prompt is required.".into());
    }

    let max_tokens = req.max_tokens.unwrap_or(48).clamp(8, 128) as usize;

    tauri::async_runtime::spawn_blocking(move || {
        run_probe_blocking(&model_path, model_filename, prompt, max_tokens)
    })
    .await
    .map_err(|err| err.to_string())?
}

fn run_probe_blocking(
    model_path: &Path,
    model_filename: String,
    prompt: String,
    max_tokens: usize,
) -> Result<ProbeResult, String> {
    let backend = llama_backend()?;
    let load_started = Instant::now();

    let model_params = LlamaModelParams::default()
        .with_n_gpu_layers(0)
        .with_use_mmap(true);
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
        .map_err(|err| err.to_string())?;

    let thread_count = std::thread::available_parallelism()
        .map(|value| value.get().clamp(1, 4) as i32)
        .unwrap_or(2);
    let context_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(1024))
        .with_n_batch(512)
        .with_n_threads(thread_count)
        .with_n_threads_batch(thread_count);
    let mut context = model
        .new_context(&backend, context_params)
        .map_err(|err| err.to_string())?;

    let (rendered_prompt, used_chat_template, add_bos) = build_prompt(&model, &prompt)?;
    let prompt_tokens = model
        .str_to_token(
            &rendered_prompt,
            if add_bos {
                AddBos::Always
            } else {
                AddBos::Never
            },
        )
        .map_err(|err| err.to_string())?;
    if prompt_tokens.is_empty() {
        return Err("Prompt rendered to zero tokens.".into());
    }

    let mut batch = LlamaBatch::new(prompt_tokens.len() + 1, 1);
    batch
        .add_sequence(&prompt_tokens, 0, false)
        .map_err(|err| err.to_string())?;
    context.decode(&mut batch).map_err(|err| err.to_string())?;

    let load_ms = load_started.elapsed().as_millis() as u64;
    let generation_started = Instant::now();
    let mut sampler = LlamaSampler::greedy();
    let eos_token = model.token_eos();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut output = String::new();
    let mut generated_tokens = 0usize;
    let mut first_token_ms = None;
    let mut position = i32::try_from(prompt_tokens.len()).map_err(|err| err.to_string())?;

    while generated_tokens < max_tokens {
        let token = sampler.sample(&context, 0);
        sampler.accept(token);

        if token == eos_token {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .map_err(|err| err.to_string())?;

        if first_token_ms.is_none() && !piece.trim().is_empty() {
            first_token_ms = Some(generation_started.elapsed().as_millis() as u64);
        }

        output.push_str(&piece);
        generated_tokens += 1;

        if output.trim_end().ends_with('?') {
            break;
        }

        batch.clear();
        batch
            .add(token, position, &[0], true)
            .map_err(|err| err.to_string())?;
        position += 1;
        context.decode(&mut batch).map_err(|err| err.to_string())?;
    }

    let output = output.trim().to_string();
    if output.is_empty() {
        return Err("Model generated no visible text.".into());
    }

    let devices = list_llama_ggml_backend_devices()
        .into_iter()
        .map(|device| {
            format!(
                "{} / {} / {}",
                device.backend, device.name, device.description
            )
        })
        .collect();

    Ok(ProbeResult {
        model_filename,
        model_path: model_path.to_string_lossy().into_owned(),
        prompt,
        output,
        used_chat_template,
        prompt_tokens: prompt_tokens.len(),
        generated_tokens,
        load_ms,
        first_token_ms,
        total_ms: generation_started.elapsed().as_millis() as u64,
        devices,
    })
}

fn llama_backend() -> Result<&'static LlamaBackend, String> {
    if let Some(backend) = BACKEND.get() {
        return Ok(backend);
    }

    let _guard = BACKEND_INIT_LOCK
        .lock()
        .map_err(|_| "Llama backend init lock was poisoned.".to_string())?;

    if let Some(backend) = BACKEND.get() {
        return Ok(backend);
    }

    let backend = LlamaBackend::init().map_err(|err| err.to_string())?;
    BACKEND
        .set(backend)
        .map_err(|_| "Llama backend was initialized concurrently.".to_string())?;

    BACKEND
        .get()
        .ok_or_else(|| "Llama backend was not available after init.".to_string())
}

fn build_prompt(model: &LlamaModel, prompt: &str) -> Result<(String, bool, bool), String> {
    let messages_json = json!([
        {
            "role": "system",
            "content": SYSTEM_PROMPT
        },
        {
            "role": "user",
            "content": prompt
        }
    ])
    .to_string();

    if let Ok(template) = model.chat_template(None) {
        let params = OpenAIChatTemplateParams {
            messages_json: &messages_json,
            tools_json: None,
            tool_choice: None,
            json_schema: None,
            grammar: None,
            reasoning_format: None,
            chat_template_kwargs: None,
            add_generation_prompt: true,
            use_jinja: true,
            parallel_tool_calls: false,
            enable_thinking: false,
            add_bos: false,
            add_eos: false,
            parse_tool_calls: false,
        };

        let rendered = model
            .apply_chat_template_oaicompat(&template, &params)
            .map_err(|err| err.to_string())?;
        return Ok((rendered.prompt, true, false));
    }

    Ok((
        format!("System: {SYSTEM_PROMPT}\nUser: {prompt}\nAssistant:"),
        false,
        true,
    ))
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?
        .join("models");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn normalize_filename(raw: &str) -> String {
    let leaf = raw
        .rsplit('/')
        .next()
        .unwrap_or(raw)
        .rsplit('\\')
        .next()
        .unwrap_or(raw)
        .split('?')
        .next()
        .unwrap_or(raw)
        .split('#')
        .next()
        .unwrap_or(raw);

    let sanitized = leaf
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string();

    if sanitized.is_empty() {
        DEFAULT_MODEL_FILENAME.to_string()
    } else {
        sanitized
    }
}
