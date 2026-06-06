use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      run_analysis_on_folder,
      run_gitnexus_status,
      run_gitnexus_analyze,
      run_gitnexus_query,
      list_ai_provider_configs,
      save_ai_provider_config,
      delete_ai_provider_config,
      run_ai_gitnexus_query
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
fn run_analysis_on_folder(
  folder_path: String,
  enable_external_tools: Option<bool>,
  selected_tools: Option<Vec<String>>,
) -> Result<String, String> {
  if let Some(selected_tools) = selected_tools {
    return ai_code_analyzer::run_analysis_on_folder_with_selected_tools(
      &folder_path,
      &selected_tools,
    );
  }

  ai_code_analyzer::run_analysis_on_folder_with_external_tools(
    &folder_path,
    enable_external_tools.unwrap_or(true),
  )
}

#[tauri::command]
fn run_gitnexus_status(folder_path: String) -> Result<String, String> {
  ai_code_analyzer::run_gitnexus_status_on_folder(&folder_path)
}

#[tauri::command]
fn run_gitnexus_analyze(folder_path: String, force: Option<bool>) -> Result<String, String> {
  ai_code_analyzer::run_gitnexus_analyze_on_folder(&folder_path, force.unwrap_or(false))
}

#[tauri::command]
fn run_gitnexus_query(
  folder_path: String,
  query: String,
  limit: Option<usize>,
) -> Result<String, String> {
  ai_code_analyzer::run_gitnexus_query_on_folder(&folder_path, &query, limit.unwrap_or(8))
}

#[tauri::command]
fn list_ai_provider_configs(app: AppHandle) -> Result<String, String> {
  let conn = open_settings_db(&app)?;
  let views: Vec<_> = provider_presets()
    .into_iter()
    .map(|preset| provider_view(&conn, preset))
    .collect::<Result<_, _>>()?;
  serde_json::to_string_pretty(&views).map_err(|e| format!("Provider JSON hatası: {}", e))
}

#[tauri::command]
fn save_ai_provider_config(
  app: AppHandle,
  provider: String,
  api_key: String,
  model: String,
  base_url: String,
) -> Result<String, String> {
  let preset = provider_preset(&provider)?;
  let conn = open_settings_db(&app)?;
  let existing = load_provider_config(&conn, &provider)?;
  let api_key = if api_key.trim().is_empty() {
    existing
      .as_ref()
      .map(|config| config.api_key.clone())
      .unwrap_or_default()
  } else {
    api_key.trim().to_string()
  };

  if api_key.is_empty() {
    return Err("API anahtarı boş olamaz.".to_string());
  }

  let model = if model.trim().is_empty() {
    preset.default_model.to_string()
  } else {
    model.trim().to_string()
  };
  let base_url = if base_url.trim().is_empty() {
    preset.default_base_url.to_string()
  } else {
    base_url.trim().trim_end_matches('/').to_string()
  };

  conn.execute(
    "INSERT INTO ai_provider_configs(provider, api_key, model, base_url, updated_at)
     VALUES (?1, ?2, ?3, ?4, datetime('now'))
     ON CONFLICT(provider) DO UPDATE SET
       api_key = excluded.api_key,
       model = excluded.model,
       base_url = excluded.base_url,
       updated_at = excluded.updated_at",
    params![provider, api_key, model, base_url],
  )
  .map_err(|e| format!("Provider kaydedilemedi: {}", e))?;

  list_ai_provider_configs(app)
}

#[tauri::command]
fn delete_ai_provider_config(app: AppHandle, provider: String) -> Result<String, String> {
  let conn = open_settings_db(&app)?;
  conn.execute(
    "DELETE FROM ai_provider_configs WHERE provider = ?1",
    params![provider],
  )
  .map_err(|e| format!("Provider silinemedi: {}", e))?;
  list_ai_provider_configs(app)
}

#[tauri::command]
fn run_ai_gitnexus_query(
  app: AppHandle,
  folder_path: String,
  provider: String,
  prompt: String,
) -> Result<String, String> {
  let prompt = prompt.trim();
  if prompt.is_empty() {
    return Err("AI/GitNexus konusu boş olamaz.".to_string());
  }

  let conn = open_settings_db(&app)?;
  let config = load_provider_config(&conn, &provider)?
    .ok_or_else(|| "Bu provider için API anahtarı kaydedilmemiş.".to_string())?;

  let gitnexus_output = ai_code_analyzer::run_gitnexus_query_on_folder(&folder_path, prompt, 12)?;
  let system_prompt = "PyShield AI Workbench içindesin. GitNexus çıktısında olmayan node, dosya veya fonksiyonları uydurma. Çıktı boşsa ya da warning içeriyorsa bunu açıkça söyle. Türkçe, kısa, teknik ve aksiyon odaklı yanıt ver: ilgili node/dosya adayları, risk, sonraki GitNexus sorguları.";
  let user_prompt = format!(
    "Kullanıcının konusu:\n{}\n\nGitNexus query çıktısı:\n{}\n\nBu çıktıyı analiz et ve PyShield kullanıcısına uygulanabilir bir özet ver.",
    prompt,
    truncate_for_model(&gitnexus_output, 16_000)
  );

  let answer = call_provider(&config, system_prompt, &user_prompt)?;
  let result = AiGitNexusResult {
    provider: config.provider,
    model: config.model,
    gitnexus_query: prompt.to_string(),
    gitnexus_output: gitnexus_output.clone(),
    gitnexus_json: serde_json::from_str::<Value>(&gitnexus_output).ok(),
    answer,
  };

  serde_json::to_string_pretty(&result).map_err(|e| format!("AI yanıt JSON hatası: {}", e))
}

#[derive(Clone, Copy)]
enum ProviderApi {
  OpenAiResponses,
  ChatCompletions,
}

#[derive(Clone, Copy)]
struct ProviderPreset {
  id: &'static str,
  label: &'static str,
  default_base_url: &'static str,
  default_model: &'static str,
  models: &'static [&'static str],
  api: ProviderApi,
  notes: &'static str,
}

#[derive(Clone)]
struct StoredProviderConfig {
  provider: String,
  api_key: String,
  model: String,
  base_url: String,
}

#[derive(Serialize)]
struct AiProviderView {
  id: String,
  label: String,
  base_url: String,
  model: String,
  models: Vec<String>,
  saved: bool,
  notes: String,
}

#[derive(Serialize)]
struct AiGitNexusResult {
  provider: String,
  model: String,
  gitnexus_query: String,
  gitnexus_output: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  gitnexus_json: Option<Value>,
  answer: String,
}

fn provider_presets() -> Vec<ProviderPreset> {
  vec![
    ProviderPreset {
      id: "openai",
      label: "OpenAI (ChatGPT API)",
      default_base_url: "https://api.openai.com/v1",
      default_model: "gpt-5.5",
      models: &["gpt-5.5", "gpt-5.4-mini", "gpt-5.4-nano"],
      api: ProviderApi::OpenAiResponses,
      notes: "Responses API ile çağrılır.",
    },
    ProviderPreset {
      id: "deepseek",
      label: "DeepSeek",
      default_base_url: "https://api.deepseek.com",
      default_model: "deepseek-v4-flash",
      models: &[
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "deepseek-chat",
        "deepseek-reasoner",
      ],
      api: ProviderApi::ChatCompletions,
      notes: "OpenAI uyumlu chat completions endpoint'i ile çağrılır.",
    },
    ProviderPreset {
      id: "openai_compatible",
      label: "OpenAI-Compatible",
      default_base_url: "https://api.openai.com/v1",
      default_model: "gpt-5.4-mini",
      models: &[
        "gpt-5.4-mini",
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "deepseek-chat",
        "deepseek-reasoner",
      ],
      api: ProviderApi::ChatCompletions,
      notes: "OpenAI uyumlu /chat/completions endpoint'i beklenir.",
    },
  ]
}

fn provider_preset(id: &str) -> Result<ProviderPreset, String> {
  provider_presets()
    .into_iter()
    .find(|preset| preset.id == id)
    .ok_or_else(|| format!("Bilinmeyen provider: {}", id))
}

fn provider_view(conn: &Connection, preset: ProviderPreset) -> Result<AiProviderView, String> {
  let stored = load_provider_config(conn, preset.id)?;
  Ok(AiProviderView {
    id: preset.id.to_string(),
    label: preset.label.to_string(),
    base_url: stored
      .as_ref()
      .map(|config| config.base_url.clone())
      .unwrap_or_else(|| preset.default_base_url.to_string()),
    model: stored
      .as_ref()
      .map(|config| config.model.clone())
      .unwrap_or_else(|| preset.default_model.to_string()),
    models: preset.models.iter().map(|model| model.to_string()).collect(),
    saved: stored.is_some(),
    notes: preset.notes.to_string(),
  })
}

fn settings_db_path(app: &AppHandle) -> Result<PathBuf, String> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| format!("App data dizini bulunamadı: {}", e))?;
  fs::create_dir_all(&dir).map_err(|e| format!("App data dizini oluşturulamadı: {}", e))?;
  Ok(dir.join("settings.sqlite"))
}

fn open_settings_db(app: &AppHandle) -> Result<Connection, String> {
  let path = settings_db_path(app)?;
  let conn = Connection::open(path).map_err(|e| format!("SQLite açılamadı: {}", e))?;
  conn.execute(
    "CREATE TABLE IF NOT EXISTS ai_provider_configs (
       provider TEXT PRIMARY KEY,
       api_key TEXT NOT NULL,
       model TEXT NOT NULL,
       base_url TEXT NOT NULL,
       updated_at TEXT NOT NULL
     )",
    [],
  )
  .map_err(|e| format!("SQLite şeması oluşturulamadı: {}", e))?;
  Ok(conn)
}

fn load_provider_config(
  conn: &Connection,
  provider: &str,
) -> Result<Option<StoredProviderConfig>, String> {
  conn
    .query_row(
      "SELECT provider, api_key, model, base_url FROM ai_provider_configs WHERE provider = ?1",
      params![provider],
      |row| {
        Ok(StoredProviderConfig {
          provider: row.get(0)?,
          api_key: row.get(1)?,
          model: row.get(2)?,
          base_url: row.get(3)?,
        })
      },
    )
    .optional()
    .map_err(|e| format!("Provider okunamadı: {}", e))
}

fn call_provider(
  config: &StoredProviderConfig,
  system_prompt: &str,
  user_prompt: &str,
) -> Result<String, String> {
  let preset = provider_preset(&config.provider)?;
  match preset.api {
    ProviderApi::OpenAiResponses => call_openai_responses(config, system_prompt, user_prompt),
    ProviderApi::ChatCompletions => call_chat_completions(config, system_prompt, user_prompt),
  }
}

fn call_openai_responses(
  config: &StoredProviderConfig,
  system_prompt: &str,
  user_prompt: &str,
) -> Result<String, String> {
  let payload = json!({
    "model": config.model,
    "input": [
      { "role": "system", "content": system_prompt },
      { "role": "user", "content": user_prompt }
    ],
    "max_output_tokens": 900
  });
  let value = post_json(
    &endpoint_url(&config.base_url, "responses"),
    &config.api_key,
    payload,
  )?;
  extract_openai_response_text(&value).ok_or_else(|| {
    format!(
      "OpenAI yanıt metni çıkarılamadı: {}",
      compact_json(&value)
    )
  })
}

fn call_chat_completions(
  config: &StoredProviderConfig,
  system_prompt: &str,
  user_prompt: &str,
) -> Result<String, String> {
  let payload = json!({
    "model": config.model,
    "messages": [
      { "role": "system", "content": system_prompt },
      { "role": "user", "content": user_prompt }
    ],
    "temperature": 0.2,
    "max_tokens": 900
  });
  let value = post_json(
    &endpoint_url(&config.base_url, "chat/completions"),
    &config.api_key,
    payload,
  )?;
  value["choices"]
    .as_array()
    .and_then(|choices| choices.first())
    .and_then(|choice| choice["message"]["content"].as_str())
    .map(|text| text.trim().to_string())
    .filter(|text| !text.is_empty())
    .ok_or_else(|| {
      format!(
        "Chat completions yanıt metni çıkarılamadı: {}",
        compact_json(&value)
      )
    })
}

fn post_json(url: &str, api_key: &str, payload: Value) -> Result<Value, String> {
  let client = Client::builder()
    .timeout(Duration::from_secs(75))
    .build()
    .map_err(|e| format!("HTTP client oluşturulamadı: {}", e))?;
  let response = client
    .post(url)
    .bearer_auth(api_key)
    .json(&payload)
    .send()
    .map_err(|e| format!("Provider isteği başarısız: {}", e))?;
  let status = response.status();
  let body = response
    .text()
    .map_err(|e| format!("Provider yanıtı okunamadı: {}", e))?;
  if !status.is_success() {
    return Err(format!(
      "Provider HTTP {}: {}",
      status.as_u16(),
      compact_text(&body, 1200)
    ));
  }
  serde_json::from_str(&body).map_err(|e| format!("Provider JSON parse hatası: {}", e))
}

fn endpoint_url(base_url: &str, endpoint: &str) -> String {
  format!(
    "{}/{}",
    base_url.trim().trim_end_matches('/'),
    endpoint.trim_start_matches('/')
  )
}

fn extract_openai_response_text(value: &Value) -> Option<String> {
  if let Some(text) = value["output_text"].as_str() {
    let text = text.trim();
    if !text.is_empty() {
      return Some(text.to_string());
    }
  }

  let mut parts = Vec::new();
  if let Some(output) = value["output"].as_array() {
    for item in output {
      if let Some(content) = item["content"].as_array() {
        for part in content {
          if let Some(text) = part["text"].as_str().or_else(|| part["content"].as_str()) {
            let text = text.trim();
            if !text.is_empty() {
              parts.push(text.to_string());
            }
          }
        }
      }
    }
  }

  if parts.is_empty() {
    None
  } else {
    Some(parts.join("\n"))
  }
}

fn truncate_for_model(text: &str, max_chars: usize) -> String {
  if text.chars().count() <= max_chars {
    return text.to_string();
  }
  let mut out: String = text.chars().take(max_chars).collect();
  out.push_str("\n...[truncated]");
  out
}

fn compact_json(value: &Value) -> String {
  compact_text(&value.to_string(), 1200)
}

fn compact_text(text: &str, max_chars: usize) -> String {
  if text.chars().count() <= max_chars {
    return text.to_string();
  }
  let mut out: String = text.chars().take(max_chars).collect();
  out.push_str("...");
  out
}
