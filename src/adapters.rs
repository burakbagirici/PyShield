use crate::{
    ErrorBreakdown, FileAnalysisResult, Issue, Severity, ToolStatus, issue_from_external_tool,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PYSHIELD_TOOLS_DIR_ENV: &str = "PYSHIELD_TOOLS_DIR";

pub trait AnalyzerAdapter {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn run(&self, project_path: &str) -> Result<Vec<Issue>, String>;
}

#[derive(Clone, Debug)]
struct ToolCommand {
    program: PathBuf,
    prefix_args: Vec<String>,
}

impl ToolCommand {
    fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            prefix_args: vec![],
        }
    }

    fn output(&self, args: &[&str], cwd: &Path) -> Result<Output, String> {
        let mut command = Command::new(&self.program);
        command.current_dir(cwd);
        command.args(&self.prefix_args);
        command.args(args);
        command
            .output()
            .map_err(|e| format!("{} çalıştırılamadı: {}", self.program.display(), e))
    }
}

pub struct RuffAdapter {
    command: Option<ToolCommand>,
}

pub struct BanditAdapter {
    command: Option<ToolCommand>,
}

pub struct PyrightAdapter {
    command: Option<ToolCommand>,
}

impl RuffAdapter {
    pub fn new(project_path: &str) -> Self {
        Self {
            command: find_tool_command(project_path, "ruff"),
        }
    }
}

impl BanditAdapter {
    pub fn new(project_path: &str) -> Self {
        Self {
            command: find_tool_command(project_path, "bandit"),
        }
    }
}

impl PyrightAdapter {
    pub fn new(project_path: &str) -> Self {
        Self {
            command: find_tool_command(project_path, "pyright"),
        }
    }
}

impl AnalyzerAdapter for RuffAdapter {
    fn name(&self) -> &'static str {
        "ruff"
    }

    fn is_available(&self) -> bool {
        self.command.is_some()
    }

    fn run(&self, project_path: &str) -> Result<Vec<Issue>, String> {
        let command = self
            .command
            .as_ref()
            .ok_or_else(|| "ruff bulunamadı".to_string())?;
        let root = Path::new(project_path);
        let output = command.output(
            &["check", ".", "--output-format", "json", "--exit-zero"],
            root,
        )?;
        parse_ruff_output(root, &output.stdout)
    }
}

impl AnalyzerAdapter for BanditAdapter {
    fn name(&self) -> &'static str {
        "bandit"
    }

    fn is_available(&self) -> bool {
        self.command.is_some()
    }

    fn run(&self, project_path: &str) -> Result<Vec<Issue>, String> {
        let command = self
            .command
            .as_ref()
            .ok_or_else(|| "bandit bulunamadı".to_string())?;
        let root = Path::new(project_path);
        let output = command.output(&["-r", ".", "-f", "json", "-q"], root)?;
        parse_bandit_output(root, &output.stdout)
    }
}

impl AnalyzerAdapter for PyrightAdapter {
    fn name(&self) -> &'static str {
        "pyright"
    }

    fn is_available(&self) -> bool {
        self.command.is_some()
    }

    fn run(&self, project_path: &str) -> Result<Vec<Issue>, String> {
        let command = self
            .command
            .as_ref()
            .ok_or_else(|| "pyright bulunamadı".to_string())?;
        let root = Path::new(project_path);
        let output = command.output(&[".", "--outputjson"], root)?;
        parse_pyright_output(root, &output.stdout)
    }
}

pub fn run_external_adapters(
    project_path: &str,
    results: &mut Vec<FileAnalysisResult>,
) -> Vec<ToolStatus> {
    let adapters: Vec<Box<dyn AnalyzerAdapter>> = vec![
        Box::new(RuffAdapter::new(project_path)),
        Box::new(BanditAdapter::new(project_path)),
        Box::new(PyrightAdapter::new(project_path)),
    ];

    let mut statuses = Vec::new();
    for adapter in adapters {
        if !adapter.is_available() {
            statuses.push(ToolStatus {
                tool: adapter.name().to_string(),
                available: false,
                status: "unavailable".to_string(),
                message: Some(format!(
                    "{} PyShield araç seti, PATH veya hedef repo fallback dizinlerinde bulunamadı.",
                    adapter.name()
                )),
                issue_count: 0,
            });
            continue;
        }

        match adapter.run(project_path) {
            Ok(issues) => {
                let issue_count = merge_external_issues(results, issues);
                statuses.push(ToolStatus {
                    tool: adapter.name().to_string(),
                    available: true,
                    status: "ok".to_string(),
                    message: None,
                    issue_count,
                });
            }
            Err(e) => statuses.push(ToolStatus {
                tool: adapter.name().to_string(),
                available: true,
                status: "error".to_string(),
                message: Some(e),
                issue_count: 0,
            }),
        }
    }

    statuses
}

fn merge_external_issues(results: &mut Vec<FileAnalysisResult>, issues: Vec<Issue>) -> usize {
    let mut by_file: HashMap<String, usize> = results
        .iter()
        .enumerate()
        .map(|(idx, file)| (file.file_path.clone(), idx))
        .collect();
    let mut seen = collect_existing_issue_keys(results);
    let mut added = 0usize;

    for issue in issues {
        let key = issue_key(&issue);
        if !seen.insert(key) {
            continue;
        }

        let file_path = issue.file_path.clone();
        let idx = if let Some(idx) = by_file.get(&file_path).copied() {
            idx
        } else {
            let idx = results.len();
            results.push(FileAnalysisResult {
                file_path: file_path.clone(),
                output_buffer: String::new(),
                issues: Vec::new(),
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
            });
            by_file.insert(file_path, idx);
            idx
        };

        apply_issue_to_breakdown(&mut results[idx], &issue);
        results[idx].issues.push(issue);
        results[idx].total_errors += 1;
        added += 1;
    }

    added
}

fn collect_existing_issue_keys(results: &[FileAnalysisResult]) -> HashSet<String> {
    results
        .iter()
        .flat_map(|file| file.issues.iter())
        .map(issue_key)
        .collect()
}

fn issue_key(issue: &Issue) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        issue.file_path, issue.tool, issue.rule_id, issue.line, issue.message
    )
}

fn apply_issue_to_breakdown(file: &mut FileAnalysisResult, issue: &Issue) {
    match issue.category.as_str() {
        "Sözdizimi" => file.error_breakdown.syntax_errors += 1,
        "Halüsinasyon" => file.error_breakdown.hallucinations += 1,
        "Eksik Import" => file.error_breakdown.missing_imports += 1,
        "Tanımsız Değişken" => file.error_breakdown.undefined_variables += 1,
        "Kullanılmayan Kod" => file.error_breakdown.unused_code += 1,
        "Güvenlik İhlali" => file.error_breakdown.security_issues += 1,
        "Sonsuz Döngü" => file.error_breakdown.infinite_loops += 1,
        _ => {}
    }
}

fn find_tool_command(project_path: &str, binary: &str) -> Option<ToolCommand> {
    let root = Path::new(project_path);
    let mut candidates = Vec::new();

    candidates.extend(pyshield_managed_candidates(binary));
    candidates.extend(path_candidates(binary));
    candidates.extend(project_local_candidates(root, binary));

    if binary == "pyright" {
        candidates.extend(project_node_candidates(root, binary));
    }

    candidates.into_iter().find(|candidate| {
        candidate
            .output(&["--version"], root)
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

fn pyshield_managed_candidates(binary: &str) -> Vec<ToolCommand> {
    let mut candidates = Vec::new();
    for tools_root in pyshield_tools_roots() {
        candidates.extend(python_tool_candidates(&tools_root, binary));
        if binary == "pyright" {
            candidates.extend(managed_node_candidates(&tools_root, binary));
        }
    }
    candidates
}

fn pyshield_tools_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(explicit_root) = env::var_os(PYSHIELD_TOOLS_DIR_ENV) {
        push_unique_path(&mut roots, PathBuf::from(explicit_root));
        return roots;
    }

    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        push_unique_path(&mut roots, Path::new(manifest_dir).join(".pyshield-tools"));
    }

    if let Ok(current_dir) = env::current_dir() {
        push_unique_path(&mut roots, current_dir.join(".pyshield-tools"));
        for ancestor in current_dir.ancestors().skip(1) {
            push_unique_path(&mut roots, ancestor.join(".pyshield-tools"));
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            push_unique_path(&mut roots, exe_dir.join(".pyshield-tools"));
            for ancestor in exe_dir.ancestors().skip(1) {
                push_unique_path(&mut roots, ancestor.join(".pyshield-tools"));
            }
        }
    }

    roots
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn python_tool_candidates(tools_root: &Path, binary: &str) -> Vec<ToolCommand> {
    let script_dirs = [
        tools_root.join("python").join("Scripts"),
        tools_root.join("python").join("bin"),
    ];
    executable_candidates(script_dirs, binary)
}

fn managed_node_candidates(tools_root: &Path, binary: &str) -> Vec<ToolCommand> {
    let script_dirs = [
        tools_root.join("node").join("node_modules").join(".bin"),
        tools_root.join("node_modules").join(".bin"),
    ];
    executable_candidates(script_dirs, binary)
}

fn project_local_candidates(root: &Path, binary: &str) -> Vec<ToolCommand> {
    let script_dirs = [
        root.join(".venv").join("Scripts"),
        root.join("venv").join("Scripts"),
        root.join(".venv").join("bin"),
        root.join("venv").join("bin"),
    ];
    executable_candidates(script_dirs, binary)
}

fn project_node_candidates(root: &Path, binary: &str) -> Vec<ToolCommand> {
    executable_candidates([root.join("node_modules").join(".bin")], binary)
}

fn path_candidates(binary: &str) -> Vec<ToolCommand> {
    executable_candidates([PathBuf::new()], binary)
}

fn executable_candidates<const N: usize>(
    script_dirs: [PathBuf; N],
    binary: &str,
) -> Vec<ToolCommand> {
    let suffixes = if cfg!(windows) {
        vec![".exe", ".cmd", ".bat", ""]
    } else {
        vec![""]
    };

    let mut candidates = Vec::new();
    for dir in script_dirs {
        for suffix in &suffixes {
            candidates.push(ToolCommand::new(dir.join(format!("{}{}", binary, suffix))));
        }
    }
    candidates
}

fn normalize_path(root: &Path, path: &str) -> String {
    let candidate = Path::new(path);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    joined
        .canonicalize()
        .unwrap_or(joined)
        .to_string_lossy()
        .to_string()
}

fn snippet_for_line(file_path: &str, line: usize) -> String {
    std::fs::read_to_string(file_path)
        .ok()
        .and_then(|content| {
            content
                .lines()
                .nth(line.saturating_sub(1))
                .map(|line| line.trim().to_string())
        })
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct RuffFinding {
    filename: String,
    code: Option<String>,
    message: String,
    location: RuffLocation,
    fix: Option<RuffFix>,
}

#[derive(Deserialize)]
struct RuffLocation {
    row: usize,
    column: usize,
}

#[derive(Deserialize)]
struct RuffFix {
    message: Option<String>,
}

fn parse_ruff_output(root: &Path, bytes: &[u8]) -> Result<Vec<Issue>, String> {
    let findings: Vec<RuffFinding> =
        serde_json::from_slice(bytes).map_err(|e| format!("ruff JSON parse hatası: {}", e))?;
    Ok(findings
        .into_iter()
        .map(|finding| {
            let file_path = normalize_path(root, &finding.filename);
            let rule_id = finding.code.unwrap_or_else(|| "RUFF".to_string());
            let (category, severity) = ruff_category_and_severity(&rule_id);
            let line = finding.location.row.max(1);
            let column = finding.location.column.max(1);
            let snippet = snippet_for_line(&file_path, line);
            issue_from_external_tool(
                file_path,
                "ruff",
                rule_id,
                category,
                severity,
                line,
                column,
                finding.message,
                snippet,
                finding.fix.and_then(|fix| fix.message),
            )
        })
        .collect())
}

fn ruff_category_and_severity(rule_id: &str) -> (&'static str, Severity) {
    match rule_id {
        "F401" | "F841" => ("Kullanılmayan Kod", Severity::Warning),
        "F821" | "F822" | "F823" => ("Tanımsız Değişken", Severity::Error),
        "F403" | "F405" => ("Eksik Import", Severity::Warning),
        "E999" => ("Sözdizimi", Severity::Error),
        _ => ("Lint", Severity::Warning),
    }
}

#[derive(Deserialize)]
struct BanditReport {
    results: Vec<BanditFinding>,
}

#[derive(Deserialize)]
struct BanditFinding {
    filename: String,
    line_number: usize,
    issue_text: String,
    issue_severity: String,
    test_id: String,
    code: Option<String>,
}

fn parse_bandit_output(root: &Path, bytes: &[u8]) -> Result<Vec<Issue>, String> {
    let report: BanditReport =
        serde_json::from_slice(bytes).map_err(|e| format!("bandit JSON parse hatası: {}", e))?;
    Ok(report
        .results
        .into_iter()
        .map(|finding| {
            let file_path = normalize_path(root, &finding.filename);
            let severity = match finding.issue_severity.as_str() {
                "HIGH" | "MEDIUM" => Severity::Error,
                _ => Severity::Warning,
            };
            let line = finding.line_number.max(1);
            let snippet = finding
                .code
                .as_deref()
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| snippet_for_line(&file_path, line));
            issue_from_external_tool(
                file_path,
                "bandit",
                finding.test_id,
                "Güvenlik İhlali",
                severity,
                line,
                1,
                finding.issue_text,
                snippet,
                Some(
                    "Bandit bulgusunu inceleyin; güvenli API veya secret yönetimi kullanın."
                        .to_string(),
                ),
            )
        })
        .collect())
}

#[derive(Deserialize)]
struct PyrightReport {
    #[serde(rename = "generalDiagnostics")]
    general_diagnostics: Vec<PyrightDiagnostic>,
}

#[derive(Deserialize)]
struct PyrightDiagnostic {
    file: String,
    severity: String,
    message: String,
    rule: Option<String>,
    range: PyrightRange,
}

#[derive(Deserialize)]
struct PyrightRange {
    start: PyrightPosition,
}

#[derive(Deserialize)]
struct PyrightPosition {
    line: usize,
    character: usize,
}

fn parse_pyright_output(root: &Path, bytes: &[u8]) -> Result<Vec<Issue>, String> {
    let report: PyrightReport =
        serde_json::from_slice(bytes).map_err(|e| format!("pyright JSON parse hatası: {}", e))?;
    Ok(report
        .general_diagnostics
        .into_iter()
        .map(|diagnostic| {
            let file_path = normalize_path(root, &diagnostic.file);
            let rule_id = diagnostic.rule.unwrap_or_else(|| "PYRIGHT".to_string());
            let (category, severity) = pyright_category_and_severity(&rule_id, &diagnostic.severity);
            let line = diagnostic.range.start.line + 1;
            let column = diagnostic.range.start.character + 1;
            let snippet = snippet_for_line(&file_path, line);
            issue_from_external_tool(
                file_path,
                "pyright",
                rule_id,
                category,
                severity,
                line,
                column,
                diagnostic.message,
                snippet,
                Some("Pyright type/import teşhisini inceleyin; tip, import veya sembol tanımını düzeltin.".to_string()),
            )
        })
        .collect())
}

fn pyright_category_and_severity(rule_id: &str, severity: &str) -> (&'static str, Severity) {
    let mapped_severity = if severity.eq_ignore_ascii_case("error") {
        Severity::Error
    } else {
        Severity::Warning
    };
    match rule_id {
        "reportMissingImports" | "reportMissingModuleSource" => ("Eksik Import", mapped_severity),
        "reportUndefinedVariable" => ("Tanımsız Değişken", mapped_severity),
        _ => ("Type Checking", mapped_severity),
    }
}
