use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use rust_xlsxwriter::Workbook;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
            load_cached_analysis,
            delete_cached_analysis,
            update_issue_override,
            export_analysis_report,
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
    app: AppHandle,
    folder_path: String,
    enable_external_tools: Option<bool>,
    selected_tools: Option<Vec<String>>,
    force_rescan: Option<bool>,
) -> Result<String, String> {
    let cache_tools = cache_tools_for_selection(selected_tools.as_deref(), enable_external_tools);
    let raw = if let Some(selected_tools) = selected_tools {
        ai_code_analyzer::run_analysis_on_folder_with_selected_tools(&folder_path, &selected_tools)
    } else {
        ai_code_analyzer::run_analysis_on_folder_with_external_tools(
            &folder_path,
            enable_external_tools.unwrap_or(true),
        )
    }?;

    let report = serde_json::from_str::<Value>(&raw)
        .map_err(|e| format!("Analiz JSON parse hatası: {}", e))?;
    let conn = open_settings_db(&app)?;
    if force_rescan.unwrap_or(false) {
        clear_cached_tool_runs(&conn, &folder_path, &cache_tools)?;
    }
    store_report_tool_runs(&conn, &folder_path, &cache_tools, &report)?;
    cached_analysis_json(&conn, &folder_path)
}

#[tauri::command]
fn load_cached_analysis(app: AppHandle, folder_path: String) -> Result<String, String> {
    let conn = open_settings_db(&app)?;
    cached_analysis_json(&conn, &folder_path)
}

#[tauri::command]
fn delete_cached_analysis(
    app: AppHandle,
    folder_path: String,
    tools: Option<Vec<String>>,
) -> Result<String, String> {
    if folder_path.trim().is_empty() {
        return Err("Silme için proje path boş olamaz.".to_string());
    }
    let cache_tools = cache_tools_for_selection(tools.as_deref(), Some(true));
    if cache_tools.is_empty() {
        return Err("Silinecek analiz aracı seçilmedi.".to_string());
    }

    let conn = open_settings_db(&app)?;
    clear_cached_tool_runs(&conn, &folder_path, &cache_tools)?;
    cached_analysis_json(&conn, &folder_path)
}

#[tauri::command]
fn update_issue_override(
    app: AppHandle,
    folder_path: String,
    issue_key: String,
    status: Option<String>,
    note: Option<String>,
    hidden: Option<bool>,
) -> Result<String, String> {
    if folder_path.trim().is_empty() {
        return Err("Issue güncelleme için proje path boş olamaz.".to_string());
    }
    if issue_key.trim().is_empty() {
        return Err("Issue güncelleme için issue key boş olamaz.".to_string());
    }

    let conn = open_settings_db(&app)?;
    upsert_issue_override(&conn, &folder_path, &issue_key, status, note, hidden)?;
    cached_analysis_json(&conn, &folder_path)
}

#[tauri::command]
fn export_analysis_report(
    app: AppHandle,
    folder_path: String,
    format: String,
    issue_keys: Option<Vec<String>>,
) -> Result<String, String> {
    let conn = open_settings_db(&app)?;
    let report = combined_cached_report(&conn, &folder_path)?;
    if report.is_null() {
        return Err("Export için önce en az bir analiz çalıştırın.".to_string());
    }

    let mut rows = export_rows(&report);
    let selected_keys: BTreeSet<String> = issue_keys
        .unwrap_or_default()
        .into_iter()
        .filter(|key| !key.trim().is_empty())
        .collect();
    if !selected_keys.is_empty() {
        rows.retain(|row| selected_keys.contains(&row.issue_key));
    }
    if rows.is_empty() {
        return Err("Export edilecek bulgu yok.".to_string());
    }

    let dir = export_dir(&app)?;

    let stamp = unix_stamp();
    match format.trim().to_ascii_lowercase().as_str() {
        "csv" => {
            let path = dir.join(format!("pyshield-analysis-{}.csv", stamp));
            write_csv_export(&path, &rows)?;
            Ok(path.to_string_lossy().to_string())
        }
        "excel" | "xlsx" => {
            let path = dir.join(format!("pyshield-analysis-{}.xlsx", stamp));
            write_xlsx_export(&path, &rows)?;
            Ok(path.to_string_lossy().to_string())
        }
        other => Err(format!("Bilinmeyen export formatı: {}", other)),
    }
}

fn export_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = match app.path().download_dir() {
        Ok(dir) => dir,
        Err(_) => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Export dizini bulunamadı: {}", e))?
            .join("exports"),
    };
    fs::create_dir_all(&dir).map_err(|e| format!("Export dizini oluşturulamadı: {}", e))?;
    Ok(dir)
}

#[tauri::command]
fn run_gitnexus_status(app: AppHandle, folder_path: String) -> Result<String, String> {
    let raw = ai_code_analyzer::run_gitnexus_status_on_folder(&folder_path)?;
    let status = serde_json::from_str::<Value>(&raw)
        .map_err(|e| format!("GitNexus status JSON parse hatası: {}", e))?;
    let conn = open_settings_db(&app)?;
    store_status_only_report(&conn, &folder_path, "gitnexus", status)?;
    Ok(raw)
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

#[derive(Clone)]
struct CachedToolRun {
    tool: String,
    report: Value,
    updated_at: String,
}

#[derive(Clone)]
struct IssueOverride {
    status: String,
    note: String,
    hidden: bool,
    updated_at: String,
}

#[derive(Clone)]
struct SummaryCounts {
    syntax_errors: usize,
    hallucinations: usize,
    missing_imports: usize,
    undefined_variables: usize,
    unused_code: usize,
    security_issues: usize,
    infinite_loops: usize,
}

impl SummaryCounts {
    fn new() -> Self {
        Self {
            syntax_errors: 0,
            hallucinations: 0,
            missing_imports: 0,
            undefined_variables: 0,
            unused_code: 0,
            security_issues: 0,
            infinite_loops: 0,
        }
    }

    fn add_category(&mut self, category: &str) {
        match category {
            "Sözdizimi" => self.syntax_errors += 1,
            "Halüsinasyon" => self.hallucinations += 1,
            "Eksik Import" => self.missing_imports += 1,
            "Tanımsız Değişken" => self.undefined_variables += 1,
            "Kullanılmayan Kod" => self.unused_code += 1,
            "Güvenlik İhlali" => self.security_issues += 1,
            "Sonsuz Döngü" => self.infinite_loops += 1,
            other if other.contains("Import") => self.missing_imports += 1,
            other if other.contains("Halüsinasyon") => self.hallucinations += 1,
            other if other.contains("Tanımlanmamış") => self.undefined_variables += 1,
            other if other.contains("Güvenlik") => self.security_issues += 1,
            other if other.contains("Döngü") => self.infinite_loops += 1,
            _ => {}
        }
    }

    fn merge(&mut self, other: &SummaryCounts) {
        self.syntax_errors += other.syntax_errors;
        self.hallucinations += other.hallucinations;
        self.missing_imports += other.missing_imports;
        self.undefined_variables += other.undefined_variables;
        self.unused_code += other.unused_code;
        self.security_issues += other.security_issues;
        self.infinite_loops += other.infinite_loops;
    }

    fn to_json(&self) -> Value {
        json!({
          "syntax_errors": self.syntax_errors,
          "hallucinations": self.hallucinations,
          "missing_imports": self.missing_imports,
          "undefined_variables": self.undefined_variables,
          "unused_code": self.unused_code,
          "security_issues": self.security_issues,
          "infinite_loops": self.infinite_loops
        })
    }
}

#[derive(Clone)]
struct ExportIssueRow {
    issue_key: String,
    workflow_status: String,
    review_note: String,
    severity: String,
    priority: String,
    category: String,
    tool: String,
    rule_id: String,
    file_path: String,
    line: String,
    column: String,
    message: String,
    code_snippet: String,
    fix_hint: String,
    confidence: String,
    strength: usize,
    common_tools: String,
    common: bool,
    support_key: String,
}

fn cache_tools_for_selection(
    selected_tools: Option<&[String]>,
    enable_external_tools: Option<bool>,
) -> Vec<String> {
    let selected: Vec<String> = selected_tools
        .unwrap_or(&[])
        .iter()
        .map(|tool| canonical_tool_name(tool))
        .filter(|tool| !tool.is_empty())
        .collect();

    if selected.iter().any(|tool| tool == "full") {
        return all_cache_tools();
    }

    if !selected.is_empty() {
        return unique_tools(
            selected
                .into_iter()
                .filter_map(|tool| {
                    if all_cache_tools().contains(&tool) {
                        Some(tool)
                    } else {
                        None
                    }
                })
                .collect(),
        );
    }

    if enable_external_tools.unwrap_or(true) {
        all_cache_tools()
    } else {
        vec!["pyshield".to_string()]
    }
}

fn all_cache_tools() -> Vec<String> {
    vec![
        "pyshield".to_string(),
        "ruff".to_string(),
        "bandit".to_string(),
        "pyright".to_string(),
        "gitnexus".to_string(),
    ]
}

fn unique_tools(tools: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tools
        .into_iter()
        .filter(|tool| seen.insert(tool.clone()))
        .collect()
}

fn canonical_tool_name(tool: &str) -> String {
    match tool.trim().to_ascii_lowercase().as_str() {
        "native" => "pyshield".to_string(),
        other => other.to_string(),
    }
}

fn clear_cached_tool_runs(
    conn: &Connection,
    project_path: &str,
    tools: &[String],
) -> Result<(), String> {
    for tool in tools {
        conn.execute(
            "DELETE FROM analysis_tool_runs WHERE project_path = ?1 AND tool = ?2",
            params![project_path, tool],
        )
        .map_err(|e| format!("Analiz cache temizlenemedi: {}", e))?;
    }
    Ok(())
}

fn store_report_tool_runs(
    conn: &Connection,
    project_path: &str,
    tools: &[String],
    report: &Value,
) -> Result<(), String> {
    for tool in tools {
        let tool_report = tool_specific_report(report, tool);
        let report_json = serde_json::to_string(&tool_report)
            .map_err(|e| format!("Tool raporu serileştirilemedi: {}", e))?;
        conn.execute(
            "INSERT INTO analysis_tool_runs(project_path, tool, report_json, updated_at)
         VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(project_path, tool) DO UPDATE SET
           report_json = excluded.report_json,
           updated_at = excluded.updated_at",
            params![project_path, tool, report_json],
        )
        .map_err(|e| format!("Analiz cache yazılamadı: {}", e))?;
    }
    Ok(())
}

fn store_status_only_report(
    conn: &Connection,
    project_path: &str,
    tool: &str,
    status: Value,
) -> Result<(), String> {
    let report = json!({
      "analysis_timestamp": "",
      "project_path": project_path,
      "total_files_scanned": 0,
      "total_errors_found": 0,
      "analysis_status": "✅ CLEAN",
      "files": [],
      "summary": SummaryCounts::new().to_json(),
      "tool_statuses": [status]
    });
    store_report_tool_runs(conn, project_path, &[tool.to_string()], &report)
}

fn cached_analysis_json(conn: &Connection, folder_path: &str) -> Result<String, String> {
    let report = combined_cached_report(conn, folder_path)?;
    serde_json::to_string_pretty(&report).map_err(|e| format!("Cache JSON hatası: {}", e))
}

fn load_cached_tool_runs(
    conn: &Connection,
    project_path: &str,
) -> Result<Vec<CachedToolRun>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT tool, report_json, updated_at
       FROM analysis_tool_runs
       WHERE project_path = ?1
       ORDER BY updated_at ASC, tool ASC",
        )
        .map_err(|e| format!("Analiz cache okunamadı: {}", e))?;
    let rows = stmt
        .query_map(params![project_path], |row| {
            let tool: String = row.get(0)?;
            let report_json: String = row.get(1)?;
            let updated_at: String = row.get(2)?;
            Ok((tool, report_json, updated_at))
        })
        .map_err(|e| format!("Analiz cache sorgulanamadı: {}", e))?;

    let mut runs = Vec::new();
    for row in rows {
        let (tool, report_json, updated_at) =
            row.map_err(|e| format!("Analiz cache satırı okunamadı: {}", e))?;
        let report = serde_json::from_str::<Value>(&report_json)
            .map_err(|e| format!("Analiz cache JSON parse hatası: {}", e))?;
        runs.push(CachedToolRun {
            tool,
            report,
            updated_at,
        });
    }
    Ok(runs)
}

fn upsert_issue_override(
    conn: &Connection,
    project_path: &str,
    issue_key: &str,
    status: Option<String>,
    note: Option<String>,
    hidden: Option<bool>,
) -> Result<(), String> {
    let existing = load_issue_override(conn, project_path, issue_key)?;
    let current_status = existing
        .as_ref()
        .map(|override_row| override_row.status.as_str())
        .unwrap_or("open");
    let next_status = status
        .as_deref()
        .map(normalize_issue_status)
        .unwrap_or_else(|| current_status.to_string());
    let next_note = note
        .or_else(|| existing.as_ref().map(|override_row| override_row.note.clone()))
        .unwrap_or_default();
    let next_hidden = hidden.unwrap_or_else(|| existing.as_ref().is_some_and(|row| row.hidden));

    conn.execute(
        "INSERT INTO issue_overrides(project_path, issue_key, status, note, hidden, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(project_path, issue_key) DO UPDATE SET
           status = excluded.status,
           note = excluded.note,
           hidden = excluded.hidden,
           updated_at = excluded.updated_at",
        params![
            project_path,
            issue_key,
            next_status,
            next_note,
            if next_hidden { 1 } else { 0 }
        ],
    )
    .map_err(|e| format!("Issue state kaydedilemedi: {}", e))?;
    Ok(())
}

fn normalize_issue_status(status: &str) -> String {
    match status.trim().to_ascii_lowercase().as_str() {
        "completed" | "done" | "resolved" => "completed".to_string(),
        "review" | "note" | "noted" => "review".to_string(),
        _ => "open".to_string(),
    }
}

fn load_issue_override(
    conn: &Connection,
    project_path: &str,
    issue_key: &str,
) -> Result<Option<IssueOverride>, String> {
    conn.query_row(
        "SELECT status, note, hidden, updated_at
         FROM issue_overrides
         WHERE project_path = ?1 AND issue_key = ?2",
        params![project_path, issue_key],
        |row| {
            Ok(IssueOverride {
                status: row.get(0)?,
                note: row.get(1)?,
                hidden: row.get::<_, i64>(2)? != 0,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("Issue state okunamadı: {}", e))
}

fn load_issue_overrides(
    conn: &Connection,
    project_path: &str,
) -> Result<HashMap<String, IssueOverride>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT issue_key, status, note, hidden, updated_at
             FROM issue_overrides
             WHERE project_path = ?1",
        )
        .map_err(|e| format!("Issue state sorgusu hazırlanamadı: {}", e))?;
    let rows = stmt
        .query_map(params![project_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                IssueOverride {
                    status: row.get(1)?,
                    note: row.get(2)?,
                    hidden: row.get::<_, i64>(3)? != 0,
                    updated_at: row.get(4)?,
                },
            ))
        })
        .map_err(|e| format!("Issue state sorgulanamadı: {}", e))?;

    let mut overrides = HashMap::new();
    for row in rows {
        let (issue_key, override_row) =
            row.map_err(|e| format!("Issue state satırı okunamadı: {}", e))?;
        overrides.insert(issue_key, override_row);
    }
    Ok(overrides)
}

fn apply_issue_overrides(report: &mut Value, overrides: &HashMap<String, IssueOverride>) {
    if overrides.is_empty() {
        add_default_issue_state(report);
        return;
    }

    if let Some(files) = report.get_mut("files").and_then(Value::as_array_mut) {
        for file in files {
            let fallback_path = file
                .get("file_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let Some(issues) = file.get_mut("issues").and_then(Value::as_array_mut) else {
                continue;
            };

            let mut filtered = Vec::new();
            for mut issue in std::mem::take(issues) {
                if issue
                    .get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                {
                    issue["file_path"] = Value::String(fallback_path.clone());
                }

                let key = issue_key(&issue);
                issue["issue_key"] = Value::String(key.clone());
                if let Some(override_row) = overrides.get(&key) {
                    if override_row.hidden {
                        continue;
                    }
                    issue["workflow_status"] = Value::String(override_row.status.clone());
                    issue["review_note"] = Value::String(override_row.note.clone());
                    issue["workflow_updated_at"] = Value::String(override_row.updated_at.clone());
                } else {
                    issue["workflow_status"] = Value::String("open".to_string());
                    issue["review_note"] = Value::String(String::new());
                }
                filtered.push(issue);
            }
            *issues = filtered;
        }
    }
    recompute_report(report);
}

fn add_default_issue_state(report: &mut Value) {
    if let Some(files) = report.get_mut("files").and_then(Value::as_array_mut) {
        for file in files {
            let fallback_path = file
                .get("file_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for issue in file
                .get_mut("issues")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if issue
                    .get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                {
                    issue["file_path"] = Value::String(fallback_path.clone());
                }
                issue["issue_key"] = Value::String(issue_key(issue));
                issue["workflow_status"] = Value::String("open".to_string());
                issue["review_note"] = Value::String(String::new());
            }
        }
    }
}

fn combined_cached_report(conn: &Connection, project_path: &str) -> Result<Value, String> {
    let runs = load_cached_tool_runs(conn, project_path)?;
    if runs.is_empty() {
        return Ok(Value::Null);
    }
    let mut report = combine_tool_runs(project_path, &runs);
    let overrides = load_issue_overrides(conn, project_path)?;
    apply_issue_overrides(&mut report, &overrides);
    Ok(report)
}

fn combine_tool_runs(project_path: &str, runs: &[CachedToolRun]) -> Value {
    let mut files_by_path: BTreeMap<String, Value> = BTreeMap::new();
    let mut seen_issues = HashSet::new();
    let mut statuses_by_tool: BTreeMap<String, Value> = BTreeMap::new();
    let mut cached_tools = Vec::new();
    let mut latest = String::new();

    for run in runs {
        latest = run.updated_at.clone();
        cached_tools.push(json!({
          "tool": run.tool,
          "updated_at": run.updated_at,
        }));

        for status in run
            .report
            .get("tool_statuses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let tool = status
                .get("tool")
                .and_then(Value::as_str)
                .map(canonical_tool_name)
                .unwrap_or_else(|| run.tool.clone());
            statuses_by_tool.insert(tool, status.clone());
        }

        if run.tool == "pyshield" && !statuses_by_tool.contains_key("pyshield") {
            statuses_by_tool.insert(
                "pyshield".to_string(),
                json!({
                  "tool": "pyshield",
                  "available": true,
                  "status": "ok",
                  "message": "Native Rust analyzer cache",
                  "issue_count": 0
                }),
            );
        }

        for file in run
            .report
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let file_path = file
                .get("file_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if file_path.is_empty() {
                continue;
            }
            let entry = files_by_path.entry(file_path.clone()).or_insert_with(|| {
                json!({
                  "file_path": file_path,
                  "output_buffer": "",
                  "issues": [],
                  "total_errors": 0,
                  "error_breakdown": SummaryCounts::new().to_json()
                })
            });
            if entry
                .get("output_buffer")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty()
            {
                if let Some(output) = file.get("output_buffer").and_then(Value::as_str) {
                    entry["output_buffer"] = Value::String(output.to_string());
                }
            }
            let Some(issues) = entry.get_mut("issues").and_then(Value::as_array_mut) else {
                continue;
            };
            for issue in file
                .get("issues")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let key = issue_key(issue);
                if seen_issues.insert(key) {
                    issues.push(issue.clone());
                }
            }
        }
    }

    let files: Vec<Value> = files_by_path.into_values().collect();
    let mut report = json!({
      "analysis_timestamp": latest,
      "project_path": project_path,
      "total_files_scanned": files.len(),
      "total_errors_found": 0,
      "analysis_status": "✅ CLEAN",
      "files": files,
      "summary": SummaryCounts::new().to_json(),
      "tool_statuses": statuses_by_tool.into_values().collect::<Vec<_>>(),
      "cached_tools": cached_tools
    });
    recompute_report(&mut report);
    report
}

fn tool_specific_report(report: &Value, tool: &str) -> Value {
    let canonical_tool = canonical_tool_name(tool);
    let keep_empty_files = canonical_tool == "pyshield";
    let mut files = Vec::new();

    for file in report
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let filtered_issues: Vec<Value> = file
            .get("issues")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|issue| {
                issue
                    .get("tool")
                    .and_then(Value::as_str)
                    .map(canonical_tool_name)
                    .unwrap_or_else(|| "pyshield".to_string())
                    == canonical_tool
            })
            .cloned()
            .collect();

        if filtered_issues.is_empty() && !keep_empty_files {
            continue;
        }

        let mut file_copy = file.clone();
        file_copy["issues"] = Value::Array(filtered_issues);
        files.push(file_copy);
    }

    let mut tool_statuses: Vec<Value> = report
        .get("tool_statuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|status| {
            status
                .get("tool")
                .and_then(Value::as_str)
                .map(canonical_tool_name)
                .unwrap_or_default()
                == canonical_tool
        })
        .cloned()
        .collect();

    if canonical_tool == "pyshield" {
        tool_statuses.push(json!({
          "tool": "pyshield",
          "available": true,
          "status": "ok",
          "message": format!("{} dosya native scan", files.len()),
          "issue_count": files
            .iter()
            .map(|file| file.get("issues").and_then(Value::as_array).map_or(0, Vec::len))
            .sum::<usize>()
        }));
    }

    let mut tool_report = json!({
      "analysis_timestamp": report.get("analysis_timestamp").cloned().unwrap_or(Value::String(String::new())),
      "project_path": report.get("project_path").cloned().unwrap_or(Value::String(String::new())),
      "total_files_scanned": files.len(),
      "total_errors_found": 0,
      "analysis_status": "✅ CLEAN",
      "files": files,
      "summary": SummaryCounts::new().to_json(),
      "tool_statuses": tool_statuses
    });
    recompute_report(&mut tool_report);
    tool_report
}

fn recompute_report(report: &mut Value) {
    let mut summary = SummaryCounts::new();
    let mut total_issues = 0usize;
    let mut has_error = false;

    if let Some(files) = report.get_mut("files").and_then(Value::as_array_mut) {
        for file in &mut *files {
            let mut file_summary = SummaryCounts::new();
            let mut file_issue_count = 0usize;
            if let Some(issues) = file.get("issues").and_then(Value::as_array) {
                for issue in issues {
                    file_issue_count += 1;
                    file_summary
                        .add_category(issue.get("category").and_then(Value::as_str).unwrap_or(""));
                    if issue
                        .get("severity")
                        .and_then(Value::as_str)
                        .is_some_and(|severity| severity.eq_ignore_ascii_case("ERROR"))
                    {
                        has_error = true;
                    }
                }
            }
            file["total_errors"] = json!(file_issue_count);
            file["error_breakdown"] = file_summary.to_json();
            summary.merge(&file_summary);
            total_issues += file_issue_count;
        }
        report["total_files_scanned"] = json!(files.len());
    }

    report["total_errors_found"] = json!(total_issues);
    report["summary"] = summary.to_json();
    report["analysis_status"] = Value::String(if total_issues == 0 {
        "✅ CLEAN".to_string()
    } else if has_error {
        "❌ ERRORS_FOUND".to_string()
    } else {
        "⚠️ WARNINGS_FOUND".to_string()
    });

    let issue_counts = issue_counts_by_tool(report);
    if let Some(statuses) = report
        .get_mut("tool_statuses")
        .and_then(Value::as_array_mut)
    {
        for status in statuses {
            let tool = status
                .get("tool")
                .and_then(Value::as_str)
                .map(canonical_tool_name)
                .unwrap_or_default();
            status["issue_count"] = json!(issue_counts.get(&tool).copied().unwrap_or(0));
        }
    }
}

fn issue_counts_by_tool(report: &Value) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for issue in flatten_report_issues(report) {
        let tool = issue
            .get("tool")
            .and_then(Value::as_str)
            .map(canonical_tool_name)
            .unwrap_or_else(|| "pyshield".to_string());
        *counts.entry(tool).or_insert(0) += 1;
    }
    counts
}

fn flatten_report_issues(report: &Value) -> Vec<Value> {
    report
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let fallback_path = file
                .get("file_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            file.get("issues")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(move |issue| {
                    let mut issue = issue.clone();
                    if issue
                        .get("file_path")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .is_empty()
                    {
                        issue["file_path"] = Value::String(fallback_path.clone());
                    }
                    issue
                })
        })
        .collect()
}

fn issue_key(issue: &Value) -> String {
    vec![
        issue
            .get("file_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        issue
            .get("tool")
            .and_then(Value::as_str)
            .map(canonical_tool_name)
            .unwrap_or_else(|| "pyshield".to_string()),
        issue
            .get("rule_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        issue
            .get("line")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .to_string(),
        issue
            .get("column")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .to_string(),
        issue
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    ]
    .join("|")
}

fn support_key(issue: &Value) -> String {
    let file_path = issue
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('\\', "/")
        .to_ascii_lowercase();
    let line = issue.get("line").and_then(Value::as_u64).unwrap_or(0);
    let category = issue
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    format!("{}|{}|{}", file_path, line, category)
}

fn export_rows(report: &Value) -> Vec<ExportIssueRow> {
    let issues = flatten_report_issues(report);
    let mut support: HashMap<String, BTreeSet<String>> = HashMap::new();
    for issue in &issues {
        let key = support_key(issue);
        let tool = issue
            .get("tool")
            .and_then(Value::as_str)
            .map(canonical_tool_name)
            .unwrap_or_else(|| "pyshield".to_string());
        support.entry(key).or_default().insert(tool);
    }

    issues
        .into_iter()
        .map(|issue| {
            let key = support_key(&issue);
            let tools = support.get(&key).cloned().unwrap_or_default();
            let strength = tools.len().max(1);
            let common_tools = tools.into_iter().collect::<Vec<_>>().join(", ");
            let category = string_field(&issue, "category");
            let severity = string_field(&issue, "severity");
            let rule_id = string_field(&issue, "rule_id");
            ExportIssueRow {
                issue_key: issue_key(&issue),
                workflow_status: string_field(&issue, "workflow_status"),
                review_note: string_field(&issue, "review_note"),
                severity: severity.clone(),
                priority: priority_for(&category, &severity, &rule_id),
                category,
                tool: string_field(&issue, "tool"),
                rule_id,
                file_path: string_field(&issue, "file_path"),
                line: number_or_string_field(&issue, "line"),
                column: number_or_string_field(&issue, "column"),
                message: string_field(&issue, "message"),
                code_snippet: string_field(&issue, "code_snippet"),
                fix_hint: string_field(&issue, "fix_hint"),
                confidence: number_or_string_field(&issue, "confidence"),
                strength,
                common_tools,
                common: strength > 1,
                support_key: key,
            }
        })
        .collect()
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn number_or_string_field(value: &Value, key: &str) -> String {
    if let Some(text) = value.get(key).and_then(Value::as_str) {
        text.to_string()
    } else if let Some(number) = value.get(key).and_then(Value::as_u64) {
        number.to_string()
    } else if let Some(number) = value.get(key).and_then(Value::as_f64) {
        format!("{:.2}", number)
    } else {
        String::new()
    }
}

fn priority_for(category: &str, severity: &str, rule_id: &str) -> String {
    let category = category.to_ascii_lowercase();
    let severity = severity.to_ascii_uppercase();
    let rule = rule_id.to_ascii_uppercase();
    if category.contains("güvenlik")
        || category.contains("security")
        || rule.starts_with('S')
        || matches!(
            rule.as_str(),
            "B602" | "B603" | "B604" | "B605" | "B606" | "B607" | "B608"
        )
    {
        return "P0 Acil güvenlik/API".to_string();
    }
    if category.contains("sözdizimi") || category.contains("sonsuz") {
        return "P0 Çalışmayı kırar".to_string();
    }
    if category.contains("tanımsız") || category.contains("import") || severity == "ERROR" {
        return "P1 Doğruluk".to_string();
    }
    if category.contains("halüsinasyon") || category.contains("type") {
        return "P2 İnceleme".to_string();
    }
    "P3 Bakım".to_string()
}

fn write_csv_export(path: &PathBuf, rows: &[ExportIssueRow]) -> Result<(), String> {
    let headers = export_headers();
    let mut output = String::new();
    output.push_str(
        &headers
            .iter()
            .map(|item| csv_escape(item))
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push_str("\r\n");
    for row in rows {
        output.push_str(
            &export_row_values(row)
                .iter()
                .map(|item| csv_escape(item))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push_str("\r\n");
    }
    fs::write(path, output).map_err(|e| format!("CSV yazılamadı: {}", e))
}

fn write_xlsx_export(path: &PathBuf, rows: &[ExportIssueRow]) -> Result<(), String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name("issues")
        .map_err(|e| format!("XLSX sheet adı yazılamadı: {}", e))?;
    for (col, header) in export_headers().iter().enumerate() {
        worksheet
            .write_string(0, col as u16, *header)
            .map_err(|e| format!("XLSX header yazılamadı: {}", e))?;
    }
    for (idx, row) in rows.iter().enumerate() {
        for (col, value) in export_row_values(row).iter().enumerate() {
            worksheet
                .write_string((idx + 1) as u32, col as u16, value)
                .map_err(|e| format!("XLSX satırı yazılamadı: {}", e))?;
        }
    }

    let common_sheet = workbook.add_worksheet();
    common_sheet
        .set_name("common_findings")
        .map_err(|e| format!("XLSX common sheet adı yazılamadı: {}", e))?;
    for (col, header) in [
        "support_key",
        "strength",
        "common_tools",
        "file_path",
        "line",
        "category",
        "message",
    ]
    .iter()
    .enumerate()
    {
        common_sheet
            .write_string(0, col as u16, *header)
            .map_err(|e| format!("XLSX common header yazılamadı: {}", e))?;
    }
    let mut seen = HashSet::new();
    let mut row_idx = 1u32;
    for row in rows.iter().filter(|row| row.common) {
        if !seen.insert(row.support_key.clone()) {
            continue;
        }
        for (col, value) in [
            row.support_key.clone(),
            row.strength.to_string(),
            row.common_tools.clone(),
            row.file_path.clone(),
            row.line.clone(),
            row.category.clone(),
            row.message.clone(),
        ]
        .iter()
        .enumerate()
        {
            common_sheet
                .write_string(row_idx, col as u16, value)
                .map_err(|e| format!("XLSX common satırı yazılamadı: {}", e))?;
        }
        row_idx += 1;
    }

    workbook
        .save(path)
        .map_err(|e| format!("XLSX kaydedilemedi: {}", e))
}

fn export_headers() -> Vec<&'static str> {
    vec![
        "issue_key",
        "workflow_status",
        "review_note",
        "severity",
        "priority",
        "category",
        "tool",
        "rule_id",
        "file_path",
        "line",
        "column",
        "message",
        "code_snippet",
        "fix_hint",
        "confidence",
        "strength",
        "common_tools",
        "common",
    ]
}

fn export_row_values(row: &ExportIssueRow) -> Vec<String> {
    vec![
        row.issue_key.clone(),
        row.workflow_status.clone(),
        row.review_note.clone(),
        row.severity.clone(),
        row.priority.clone(),
        row.category.clone(),
        row.tool.clone(),
        row.rule_id.clone(),
        row.file_path.clone(),
        row.line.clone(),
        row.column.clone(),
        row.message.clone(),
        row.code_snippet.clone(),
        row.fix_hint.clone(),
        row.confidence.clone(),
        row.strength.to_string(),
        row.common_tools.clone(),
        row.common.to_string(),
    ]
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn unix_stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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
        models: preset
            .models
            .iter()
            .map(|model| model.to_string())
            .collect(),
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
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_provider_configs (
       provider TEXT PRIMARY KEY,
       api_key TEXT NOT NULL,
       model TEXT NOT NULL,
       base_url TEXT NOT NULL,
       updated_at TEXT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS analysis_tool_runs (
       project_path TEXT NOT NULL,
       tool TEXT NOT NULL,
       report_json TEXT NOT NULL,
       updated_at TEXT NOT NULL,
       PRIMARY KEY(project_path, tool)
     );
     CREATE TABLE IF NOT EXISTS issue_overrides (
       project_path TEXT NOT NULL,
       issue_key TEXT NOT NULL,
       status TEXT NOT NULL DEFAULT 'open',
       note TEXT NOT NULL DEFAULT '',
       hidden INTEGER NOT NULL DEFAULT 0,
       updated_at TEXT NOT NULL,
       PRIMARY KEY(project_path, issue_key)
     );",
    )
    .map_err(|e| format!("SQLite şeması oluşturulamadı: {}", e))?;
    Ok(conn)
}

fn load_provider_config(
    conn: &Connection,
    provider: &str,
) -> Result<Option<StoredProviderConfig>, String> {
    conn.query_row(
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
    extract_openai_response_text(&value)
        .ok_or_else(|| format!("OpenAI yanıt metni çıkarılamadı: {}", compact_json(&value)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_tool_runs_merge_and_mark_common_findings() {
        let pyshield_report = json!({
            "analysis_timestamp": "t1",
            "project_path": "D:\\repo",
            "files": [{
                "file_path": "D:\\repo\\sample.py",
                "output_buffer": "native",
                "issues": [{
                    "file_path": "D:\\repo\\sample.py",
                    "tool": "pyshield",
                    "rule_id": "PYSHIELD_IMPORT_001",
                    "category": "Eksik Import",
                    "severity": "ERROR",
                    "line": 4,
                    "column": 1,
                    "message": "json missing",
                    "code_snippet": "json.loads(x)",
                    "confidence": 1.0
                }]
            }],
            "tool_statuses": [{
                "tool": "pyshield",
                "available": true,
                "status": "ok",
                "issue_count": 1
            }]
        });
        let ruff_report = json!({
            "analysis_timestamp": "t2",
            "project_path": "D:\\repo",
            "files": [{
                "file_path": "D:\\repo\\sample.py",
                "issues": [{
                    "file_path": "D:\\repo\\sample.py",
                    "tool": "ruff",
                    "rule_id": "F821",
                    "category": "Eksik Import",
                    "severity": "ERROR",
                    "line": 4,
                    "column": 1,
                    "message": "Undefined name `json`",
                    "code_snippet": "json.loads(x)",
                    "confidence": 1.0
                }]
            }],
            "tool_statuses": [{
                "tool": "ruff",
                "available": true,
                "status": "ok",
                "issue_count": 1
            }]
        });
        let runs = vec![
            CachedToolRun {
                tool: "pyshield".to_string(),
                report: pyshield_report,
                updated_at: "t1".to_string(),
            },
            CachedToolRun {
                tool: "ruff".to_string(),
                report: ruff_report,
                updated_at: "t2".to_string(),
            },
        ];

        let report = combine_tool_runs("D:\\repo", &runs);
        let rows = export_rows(&report);

        assert_eq!(report["total_errors_found"], 2);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.strength == 2));
        assert!(rows.iter().all(|row| row.common));
        assert!(rows.iter().all(|row| row.common_tools.contains("pyshield")));
        assert!(rows.iter().all(|row| row.common_tools.contains("ruff")));
    }
}
