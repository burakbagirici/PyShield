mod parser;
mod rules;

use std::fs;
use std::path::Path;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use chrono::Utc;

fn should_report_error_for_line(line_number: usize, source_code: &str) -> bool {
    if let Some(line) = source_code.lines().nth(line_number) {
        !(line.contains("# noqa") || line.contains("# analiz-atla"))
    } else {
        true
    }
}

fn should_report_error_for_node(node: &tree_sitter::Node, source_code: &str) -> bool {
    should_report_error_for_line(node.start_position().row, source_code)
}

fn code_snippet_for_line(source_code: &str, line: usize) -> String {
    source_code
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim()
        .to_string()
}

fn issue_from_node(node: &tree_sitter::Node, source_code: &str, category: &str, message: String) -> Issue {
    let pos = node.start_position();
    let line = pos.row + 1;
    Issue {
        category: category.to_string(),
        line,
        column: pos.column + 1,
        message,
        code_snippet: code_snippet_for_line(source_code, line),
    }
}

fn issue_from_line(source_code: &str, category: &str, line: usize, message: String) -> Issue {
    Issue {
        category: category.to_string(),
        line,
        column: 1,
        message,
        code_snippet: code_snippet_for_line(source_code, line),
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Issue {
    pub category: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub code_snippet: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ErrorBreakdown {
    pub syntax_errors: usize,
    pub hallucinations: usize,
    pub missing_imports: usize,
    pub undefined_variables: usize,
    pub unused_code: usize,
    pub security_issues: usize,
    pub infinite_loops: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileAnalysisResult {
    pub file_path: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub output_buffer: String,
    pub issues: Vec<Issue>,
    pub total_errors: usize,
    pub error_breakdown: ErrorBreakdown,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorSummary {
    pub syntax_errors: usize,
    pub hallucinations: usize,
    pub missing_imports: usize,
    pub undefined_variables: usize,
    pub unused_code: usize,
    pub security_issues: usize,
    pub infinite_loops: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AnalysisReport {
    pub analysis_timestamp: String,
    pub project_path: String,
    pub total_files_scanned: usize,
    pub total_errors_found: usize,
    pub analysis_status: String,
    pub files: Vec<FileAnalysisResult>,
    pub summary: ErrorSummary,
}

pub fn scan_python_files_recursive(dir_path: &str) -> Result<Vec<String>, String> {
    let mut python_files = Vec::new();
    scan_directory_impl(dir_path, &mut python_files)?;
    python_files.sort();
    Ok(python_files)
}

fn scan_directory_impl(dir_path: &str, files: &mut Vec<String>) -> Result<(), String> {
    match fs::read_dir(dir_path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path.is_dir() {
                            let path_str = path.to_string_lossy().to_string();
                            if let Err(e) = scan_directory_impl(&path_str, files) {
                                eprintln!("⚠️  Alt klasör taranırken hata: {}", e);
                            }
                        } else if path.is_file() {
                            if let Some(ext) = path.extension() {
                                if ext == "py" {
                                    files.push(path.to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("⚠️  Dosya okunurken hata: {}", e),
                }
            }
            Ok(())
        }
        Err(e) => Err(format!("❌ Klasör okunamadı '{}': {}", dir_path, e)),
    }
}

fn analyze_single_file(file_path: &str) -> FileAnalysisResult {
    // Dosyayı oku (zero-copy: bir kere okuyup String'e al)
    let yapay_zeka_kodu = match fs::read_to_string(file_path) {
        Ok(içerik) => içerik,
        Err(e) => {
            return FileAnalysisResult {
                file_path: file_path.to_string(),
                output_buffer: format!("❌ Dosya okunamadı: {}\n", e),
                issues: vec![],
                total_errors: 0,
                error_breakdown: ErrorBreakdown {
                    syntax_errors: 0,
                    hallucinations: 0,
                    missing_imports: 0,
                    undefined_variables: 0,
                    unused_code: 0,
                    security_issues: 0,
                    infinite_loops: 0,
                },
            };
        }
    };

    // 1. Kodu AST'ye çeviriyoruz
    let tree = match parser::parse_python_code(&yapay_zeka_kodu) {
        Some(t) => t,
        None => {
            return FileAnalysisResult {
                file_path: file_path.to_string(),
                output_buffer: "❌ Hata: Kod ayrıştırılamadı!\n".to_string(),
                issues: vec![],
                total_errors: 0,
                error_breakdown: ErrorBreakdown {
                    syntax_errors: 0,
                    hallucinations: 0,
                    missing_imports: 0,
                    undefined_variables: 0,
                    unused_code: 0,
                    security_issues: 0,
                    infinite_loops: 0,
                },
            };
        }
    };

    let root_node = tree.root_node();
    let mut output = String::new();
    let mut issues = Vec::new();

    // Hata türlerini ayrı ayrı track et
    let mut hallucinations = 0;
    let mut missing_imports = 0;
    let mut undefined_variables = 0;
    let mut unused_code = 0;
    let mut security_issues = 0;
    let mut infinite_loops = 0;

    // 2. Sözdizimi hatalarını kontrol et
    let mut bulunan_hatalar_tumu = Vec::new();
    rules::check_for_errors(root_node, &mut bulunan_hatalar_tumu);
    
    // Muafiyet Filtresi
    let bulunan_hatalar: Vec<_> = bulunan_hatalar_tumu.into_iter()
        .filter(|node| should_report_error_for_node(node, &yapay_zeka_kodu))
        .collect();
        
    let syntax_errors = bulunan_hatalar.len();

    if !bulunan_hatalar.is_empty() {
        output.push_str("--- Sözdizimi Hataları ---\n");
        for (i, hata) in bulunan_hatalar.iter().enumerate() {
            let message = rules::format_syntax_error(hata);
            issues.push(issue_from_node(hata, &yapay_zeka_kodu, "Sözdizimi", message.clone()));
            output.push_str(&format!("{}. {}\n", i + 1, message));
        }
    } else {
        // 3. Tüm fonksiyon çağrılarını topla
        let all_function_calls = rules::collect_all_function_calls(root_node, &yapay_zeka_kodu);

        // Çağrılan fonksiyonları raportla
        output.push_str("--- Akıllı Kural: Çağrılan Fonksiyonlar Taranıyor ---\n");
        for (_node, func_name) in &all_function_calls {
            output.push_str(&format!("📌 Yapay zeka şu fonksiyonu çağırdı: {}\n", func_name));
        }

        // 4. Halüsinasyon tespiti
        let defined_functions = rules::find_function_definitions(root_node, &yapay_zeka_kodu);

        let hallucinated: Vec<(_, &str)> = all_function_calls
            .into_iter()
            .filter(|(node, func_name)| {
                let is_unrecognized = !defined_functions.contains(func_name)
                    && !rules::BUILTIN_FUNCTIONS.contains(func_name);
                is_unrecognized && should_report_error_for_node(node, &yapay_zeka_kodu)
            })
            .collect();

        hallucinations = hallucinated.len();
        if hallucinated.is_empty() {
            output.push_str("✅ Halüsinasyon tespit edilmedi!\n");
        } else {
            output.push_str(&format!("🚨 Toplam halüsinasyon: {}\n", hallucinated.len()));
            for (i, (node, name)) in hallucinated.iter().enumerate() {
                issues.push(issue_from_node(
                    node,
                    &yapay_zeka_kodu,
                    "Halüsinasyon",
                    format!("'{}' fonksiyonu tanımlı veya bilinen bir Python fonksiyonu değil.", name),
                ));
                output.push_str(&format!("   {}. {}\n", i + 1, name));
            }
        }

        // 5. Import tespiti ve eksik import kontrolü
        let imports = rules::find_all_imports(root_node, &yapay_zeka_kodu);
        output.push_str("\n📦 Kodda bulunan import'lar:\n");
        if imports.is_empty() {
            output.push_str("   (İçeri yok)\n");
        } else {
            for imp in &imports {
                output.push_str(&format!("   ✓ {}\n", imp));
            }
        }

        let mut missing_imports_vec = rules::find_missing_imports(root_node, &yapay_zeka_kodu, &imports);
        missing_imports_vec.retain(|(node, _)| should_report_error_for_node(node, &yapay_zeka_kodu));
        missing_imports = missing_imports_vec.len();
        if !missing_imports_vec.is_empty() {
            output.push_str(&format!("\n🔴 Eksik import: {} adet\n", missing_imports_vec.len()));
            for (i, (node, name)) in missing_imports_vec.iter().enumerate() {
                issues.push(issue_from_node(
                    node,
                    &yapay_zeka_kodu,
                    "Eksik Import",
                    format!("'{}' kullanılıyor ancak ilgili import bulunamadı.", name),
                ));
                output.push_str(&format!("   {}. {}\n", i + 1, name));
            }
        } else {
            output.push_str("✅ Eksik import yok!\n");
        }

        // Halüsinasyon ve eksik import isimlerini çıkar (mükerrer önleme için)
        let hallucinated_names: Vec<&str> =
            hallucinated.iter().map(|(_, name)| *name).collect();
        let missing_import_names: Vec<&str> =
            missing_imports_vec.iter().map(|(_, name)| *name).collect();

        // 6. Tanımlanmamış değişken tespiti (mükerrer filtreli)
        let mut undefined_vars = rules::find_undefined_variables(
            root_node,
            &yapay_zeka_kodu,
            &hallucinated_names,
            &missing_import_names,
        );
        undefined_vars.retain(|(node, _)| should_report_error_for_node(node, &yapay_zeka_kodu));
        undefined_variables = undefined_vars.len();
        if !undefined_vars.is_empty() {
            output.push_str(&format!("\n⚠️  Tanımlanmamış değişkenler: {} adet\n", undefined_vars.len()));
            for (i, (node, name)) in undefined_vars.iter().enumerate() {
                issues.push(issue_from_node(
                    node,
                    &yapay_zeka_kodu,
                    "Tanımsız Değişken",
                    format!("'{}' değişkeni kullanılmadan önce tanımlanmamış görünüyor.", name),
                ));
                output.push_str(&format!("   {}. {}\n", i + 1, name));
            }
        } else {
            output.push_str("✅ Tanımlanmamış değişken yok!\n");
        }

        // 7. Kullanılmayan kod tespiti
        let mut unused_code_vec = rules::find_unused_code(root_node, &yapay_zeka_kodu);
        unused_code_vec.retain(|(_, line_no, _)| should_report_error_for_line(line_no.saturating_sub(1), &yapay_zeka_kodu));
        unused_code = unused_code_vec.len();
        if !unused_code_vec.is_empty() {
            output.push_str(&format!("\n⚠️  Kullanılmayan kod: {} adet\n", unused_code_vec.len()));
            for (i, (name, line_no, kind)) in unused_code_vec.iter().enumerate() {
                issues.push(issue_from_line(
                    &yapay_zeka_kodu,
                    "Kullanılmayan Kod",
                    *line_no,
                    format!("'{}' {} olarak tanımlanmış ancak kullanılmıyor.", name, kind),
                ));
                output.push_str(&format!("   {}. {}\n", i + 1, name));
            }
        } else {
            output.push_str("✅ Kullanılmayan kod yok!\n");
        }

        // 8. Güvenlik açığı tespiti (hardcoded secrets)
        let mut secrets = rules::find_hardcoded_secrets(root_node, &yapay_zeka_kodu);
        secrets.retain(|(_, line_no, _)| should_report_error_for_line(line_no.saturating_sub(1), &yapay_zeka_kodu));
        security_issues = secrets.len();
        if !secrets.is_empty() {
            output.push_str(&format!("\n🚨 Güvenlik ihlali: {} adet\n", secrets.len()));
            for (i, (var_name, line_no, content)) in secrets.iter().enumerate() {
                issues.push(issue_from_line(
                    &yapay_zeka_kodu,
                    "Güvenlik İhlali",
                    *line_no,
                    format!("'{}' içinde hardcoded secret veya API anahtarı benzeri bir değer var.", var_name),
                ));
                output.push_str(&format!("   {}. {} = '{}...'\n", i + 1, var_name, 
                    if content.len() > 20 { &content[..20] } else { content }));
            }
        } else {
            output.push_str("✅ Güvenlik ihlali tespit edilmedi!\n");
        }

        // 8b. Tehlikeli dinamik kod yürütme tespiti (eval, exec, os.system)
        let mut dangerous_calls = rules::find_dangerous_executions(root_node, &yapay_zeka_kodu);
        dangerous_calls.retain(|(node, _)| should_report_error_for_node(node, &yapay_zeka_kodu));
        security_issues += dangerous_calls.len();
        
        if !dangerous_calls.is_empty() {
            output.push_str(&format!("\n🚨 Tehlikeli dinamik kod yürütme: {} adet\n", dangerous_calls.len()));
            for (i, (node, func_name)) in dangerous_calls.iter().enumerate() {
                let line_no = node.start_position().row + 1;
                issues.push(issue_from_node(
                    node,
                    &yapay_zeka_kodu,
                    "Güvenlik İhlali",
                    format!("'{}' dinamik kod yürütebilir; güvenlik riski oluşturur.", func_name),
                ));
                output.push_str(&format!("   {}. '{}' fonksiyonu - Satır {}\n", i + 1, func_name, line_no));
                output.push_str(&format!("      ⚠️  Problem: '{}' dinamik kod yürütebilir, güvenlik riski!\n", func_name));
            }
        } else {
            output.push_str("✅ Tehlikeli dinamik kod yürütme tespit edilmedi!\n");
        }


        // 9. Sonsuz döngü tespiti
        let mut infinite_loops_vec = rules::find_infinite_loops(root_node, &yapay_zeka_kodu);
        infinite_loops_vec.retain(|(_, line_no)| should_report_error_for_line(line_no.saturating_sub(1), &yapay_zeka_kodu));
        infinite_loops = infinite_loops_vec.len();
        if !infinite_loops_vec.is_empty() {
            output.push_str(&format!("\n⚠️  Sonsuz döngü riski: {} adet\n", infinite_loops_vec.len()));
            for (i, (condition, line_no)) in infinite_loops_vec.iter().enumerate() {
                issues.push(issue_from_line(
                    &yapay_zeka_kodu,
                    "Sonsuz Döngü",
                    *line_no,
                    format!("'while {}' çıkışsız görünüyor; break veya return bulunamadı.", condition),
                ));
                output.push_str(&format!("   {}. {}\n", i + 1, condition));
            }
        } else {
            output.push_str("✅ Sonsuz döngü tespit edilmedi!\n");
        }

        // Dosya özeti
        output.push_str("\n📋 === DOSYA ÖZETİ ===\n");
        output.push_str(&format!("Sözdizimi Hataları        : {}\n", syntax_errors));
        output.push_str(&format!("Halüsinasyon Fonksiyonları: {}\n", hallucinations));
        output.push_str(&format!("Eksik Import'lar          : {}\n", missing_imports));
        output.push_str(&format!("Tanımlanmamış Değişkenler : {}\n", undefined_variables));
        output.push_str(&format!("Kullanılmayan Kod         : {}\n", unused_code));
        output.push_str(&format!("Güvenlik İhlalleri        : {}\n", security_issues));
        output.push_str(&format!("Sonsuz Döngü Riskleri     : {}\n", infinite_loops));
        output.push_str("─────────────────────────────────\n");
        let toplam_hata = syntax_errors + hallucinations + missing_imports + undefined_variables + unused_code + security_issues + infinite_loops;
        output.push_str(&format!("Dosyadaki Toplam Sorun    : {}\n", toplam_hata));
    }

    let toplam_hata = syntax_errors + hallucinations + missing_imports + undefined_variables + unused_code + security_issues + infinite_loops;

    FileAnalysisResult {
        file_path: file_path.to_string(),
        output_buffer: output,
        issues,
        total_errors: toplam_hata,
        error_breakdown: ErrorBreakdown {
            syntax_errors,
            hallucinations,
            missing_imports,
            undefined_variables,
            unused_code,
            security_issues,
            infinite_loops,
        },
    }
}

pub fn analyze_folder(folder_path: &str) -> Result<AnalysisReport, String> {
    if !Path::new(folder_path).is_dir() {
        return Err(format!("❌ Hata: '{}' geçerli bir klasör değil!", folder_path));
    }
    let python_files = scan_python_files_recursive(folder_path)?;
    if python_files.is_empty() {
        return Ok(AnalysisReport {
            analysis_timestamp: Utc::now().to_rfc3339(),
            project_path: folder_path.to_string(),
            total_files_scanned: 0,
            total_errors_found: 0,
            analysis_status: "✅ CLEAN".to_string(),
            files: vec![],
            summary: ErrorSummary { syntax_errors: 0, hallucinations: 0, missing_imports: 0, undefined_variables: 0, unused_code: 0, security_issues: 0, infinite_loops: 0 },
        });
    }
    let results: Vec<FileAnalysisResult> = python_files.into_par_iter().map(|p| analyze_single_file(&p)).collect();
    Ok(build_report_from_results(folder_path, results))
}

fn build_report_from_results(project_path: &str, results: Vec<FileAnalysisResult>) -> AnalysisReport {
    let mut total_errors = 0usize;
    let mut s = ErrorSummary { syntax_errors: 0, hallucinations: 0, missing_imports: 0, undefined_variables: 0, unused_code: 0, security_issues: 0, infinite_loops: 0 };
    for r in &results {
        total_errors += r.total_errors;
        s.syntax_errors += r.error_breakdown.syntax_errors;
        s.hallucinations += r.error_breakdown.hallucinations;
        s.missing_imports += r.error_breakdown.missing_imports;
        s.undefined_variables += r.error_breakdown.undefined_variables;
        s.unused_code += r.error_breakdown.unused_code;
        s.security_issues += r.error_breakdown.security_issues;
        s.infinite_loops += r.error_breakdown.infinite_loops;
    }
    let status = if total_errors == 0 { "✅ CLEAN".to_string() } else { "⚠️ WARNINGS_FOUND".to_string() };
    AnalysisReport { analysis_timestamp: Utc::now().to_rfc3339(), project_path: project_path.to_string(), total_files_scanned: results.len(), total_errors_found: total_errors, analysis_status: status, files: results, summary: s }
}

pub fn run_analysis_on_folder(folder_path: &str) -> Result<String, String> {
    serde_json::to_string_pretty(&analyze_folder(folder_path)?)
        .map_err(|e| format!("JSON serileştirme hatası: {}", e))
}

pub fn generate_html_report(report: &AnalysisReport) {
    let mut html = String::with_capacity(8192);

    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>PyShield — AI Kod Analizi</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <style>
        body { background-color: #0f172a; color: #f8fafc; font-family: ui-sans-serif, system-ui, sans-serif; }
        .glass-panel { background: rgba(30, 41, 59, 0.7); backdrop-filter: blur(10px); border: 1px solid rgba(255,255,255,0.1); }
        .accordion-content { display: none; }
        .accordion-content.active { display: block; }
    </style>
    <script>
        function toggleAccordion(id) {
            const content = document.getElementById(id);
            const icon = document.getElementById('icon-' + id);
            if (content.classList.contains('active')) {
                content.classList.remove('active');
                icon.style.transform = 'rotate(0deg)';
            } else {
                content.classList.add('active');
                icon.style.transform = 'rotate(180deg)';
            }
        }
    </script>
</head>
<body class="min-h-screen p-8">
    <div class="max-w-7xl mx-auto space-y-8">
        <!-- Header -->
        <header class="flex items-center justify-between glass-panel p-6 rounded-xl shadow-lg">
            <div>
                <h1 class="text-3xl font-bold text-blue-400">PyShield</h1>
                <p class="text-slate-400 mt-1">Proje: <span class="text-white font-mono">"#);
    html.push_str(&report.project_path);
    html.push_str(r#"</span></p>
            </div>
            <div class="text-right">
                <p class="text-sm text-slate-400">Analiz Zamanı</p>
                <p class="text-white font-mono">"#);
    html.push_str(&report.analysis_timestamp);
    html.push_str(r#"</p>
            </div>
        </header>

        <!-- Özet Kartları -->
        <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
            <div class="glass-panel p-6 rounded-xl shadow-lg border-l-4 border-l-blue-500 flex flex-col justify-center">
                <h2 class="text-slate-400 text-sm uppercase tracking-wider font-semibold">Taranan Dosya</h2>
                <p class="text-4xl font-bold text-white mt-2">"#);
    html.push_str(&report.total_files_scanned.to_string());
    html.push_str(r#"</p>
            </div>
            <div class="glass-panel p-6 rounded-xl shadow-lg border-l-4 border-l-rose-500 flex flex-col justify-center">
                <h2 class="text-slate-400 text-sm uppercase tracking-wider font-semibold">Toplam Hata</h2>
                <p class="text-4xl font-bold text-white mt-2">"#);
    html.push_str(&report.total_errors_found.to_string());
    html.push_str(r#"</p>
            </div>
            <div class="glass-panel p-6 rounded-xl shadow-lg border-l-4 border-l-emerald-500 flex flex-col justify-center">
                <h2 class="text-slate-400 text-sm uppercase tracking-wider font-semibold">Durum</h2>
                <p class="text-3xl font-bold text-white mt-2">"#);
    
    let status_color = if report.total_errors_found == 0 { "text-emerald-400" } else { "text-amber-400" };
    html.push_str(&format!(r#"<span class="{}">{}</span>"#, status_color, report.analysis_status));
    
    html.push_str(r#"</p>
            </div>
        </div>

        <!-- Breakdown Alanı -->
        <div class="glass-panel p-6 rounded-xl shadow-lg">
            <h2 class="text-xl font-bold text-white mb-6 border-b border-slate-700 pb-2">Hata Dağılımı</h2>
            <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-7 gap-4">"#);

    let breakdowns = [
        ("Sözdizimi", report.summary.syntax_errors, "bg-rose-500/20 text-rose-400 border-rose-500/30"),
        ("Halüsinasyon", report.summary.hallucinations, "bg-purple-500/20 text-purple-400 border-purple-500/30"),
        ("Eksik Import", report.summary.missing_imports, "bg-amber-500/20 text-amber-400 border-amber-500/30"),
        ("Tanımsız Değ.", report.summary.undefined_variables, "bg-orange-500/20 text-orange-400 border-orange-500/30"),
        ("Kullanılmayan", report.summary.unused_code, "bg-slate-500/20 text-slate-400 border-slate-500/30"),
        ("Güvenlik", report.summary.security_issues, "bg-red-500/20 text-red-400 border-red-500/30"),
        ("Sonsuz Döngü", report.summary.infinite_loops, "bg-pink-500/20 text-pink-400 border-pink-500/30"),
    ];

    for (label, count, classes) in breakdowns.iter() {
        html.push_str(&format!(r#"
                <div class="p-4 rounded-lg border {} flex flex-col items-center justify-center text-center">
                    <span class="text-2xl font-bold mb-1">{}</span>
                    <span class="text-xs font-semibold uppercase tracking-wide">{}</span>
                </div>"#, classes, count, label));
    }

    html.push_str(r#"
            </div>
        </div>

        <!-- Dosya Listesi -->
        <div class="glass-panel rounded-xl shadow-lg overflow-hidden">
            <div class="p-6 border-b border-slate-700 bg-slate-800/50">
                <h2 class="text-xl font-bold text-white">Detaylı Dosya Raporu</h2>
            </div>
            <div class="divide-y divide-slate-700/50">"#);

    for (i, file) in report.files.iter().enumerate() {
        let has_error = file.total_errors > 0;
        let badge_class = if has_error { "bg-rose-500/20 text-rose-400 border border-rose-500/30" } else { "bg-emerald-500/20 text-emerald-400 border border-emerald-500/30" };
        let badge_text = if has_error { format!("{} Sorun", file.total_errors) } else { "Temiz".to_string() };

        html.push_str(&format!(r#"
                <div class="file-item">
                    <button onclick="toggleAccordion('file-{}')" class="w-full text-left px-6 py-4 hover:bg-slate-800/50 transition flex items-center justify-between focus:outline-none">
                        <div class="flex items-center space-x-4">
                            <span class="px-3 py-1 rounded-full text-xs font-bold {}">{}</span>
                            <span class="text-slate-200 font-mono text-sm break-all">{}</span>
                        </div>
                        <svg id="icon-file-{}" class="w-5 h-5 text-slate-400 transition-transform duration-200" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
                        </svg>
                    </button>
                    <div id="file-{}" class="accordion-content bg-slate-900/50 border-t border-slate-700/50">
                        <div class="p-6">"#, i, badge_class, badge_text, file.file_path, i, i));

        if has_error {
            let formatted_output = file.output_buffer.replace('<', "&lt;").replace('>', "&gt;");
            html.push_str(&format!(r#"
                            <div class="font-mono text-xs text-slate-300 bg-black/40 p-4 rounded-lg overflow-x-auto whitespace-pre-wrap">{}</div>"#, formatted_output));
        } else {
            html.push_str(r#"<p class="text-emerald-400 text-sm">✅ Bu dosyada herhangi bir sorun bulunamadı.</p>"#);
        }

        html.push_str(r#"
                        </div>
                    </div>
                </div>"#);
    }

    html.push_str(r#"
            </div>
        </div>
    </div>
</body>
</html>"#);

    if let Err(e) = std::fs::write("rapor.html", html) {
        eprintln!("\n❌ HTML raporu oluşturulamadı: {}", e);
    } else {
        println!("\n✅ HTML Dashboard başarıyla oluşturuldu: rapor.html");
    }
}
