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
        let mut args = vec![".".to_string(), "--outputjson".to_string()];
        if let Some(python_path) = project_python_interpreter(root) {
            args.push("--pythonpath".to_string());
            args.push(python_path);
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = command.output(&arg_refs, root)?;
        parse_pyright_output(root, &output.stdout)
    }
}

pub fn run_external_adapters(
    project_path: &str,
    results: &mut Vec<FileAnalysisResult>,
) -> Vec<ToolStatus> {
    run_external_adapters_with_tools(
        project_path,
        results,
        &["gitnexus", "ruff", "bandit", "pyright"],
    )
}

pub fn run_external_adapters_with_tools(
    project_path: &str,
    results: &mut Vec<FileAnalysisResult>,
    enabled_tools: &[&str],
) -> Vec<ToolStatus> {
    let enabled: HashSet<String> = enabled_tools
        .iter()
        .map(|tool| tool.to_ascii_lowercase())
        .collect();
    let mut statuses = Vec::new();

    if enabled.contains("gitnexus") {
        statuses.push(gitnexus_tool_status(project_path));
    }

    let adapters: Vec<Box<dyn AnalyzerAdapter>> = vec![
        Box::new(RuffAdapter::new(project_path)),
        Box::new(BanditAdapter::new(project_path)),
        Box::new(PyrightAdapter::new(project_path)),
    ];

    for adapter in adapters {
        if !enabled.contains(adapter.name()) {
            continue;
        }

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
        if matches!(binary, "pyright" | "gitnexus") {
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
    normalize_display_path(joined.canonicalize().unwrap_or(joined))
}

fn normalize_display_path(path: PathBuf) -> String {
    let value = path.to_string_lossy().to_string();
    strip_windows_verbatim_prefix(&value)
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

fn is_test_path(file_path: &str) -> bool {
    let normalized = file_path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/tests/")
        || normalized.starts_with("tests/")
        || normalized
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("test_") || name.ends_with("_test.py"))
}

pub fn gitnexus_tool_status(project_path: &str) -> ToolStatus {
    let root = Path::new(project_path);
    let Some(command) = find_tool_command(project_path, "gitnexus") else {
        return ToolStatus {
            tool: "gitnexus".to_string(),
            available: false,
            status: "unavailable".to_string(),
            message: Some("gitnexus PyShield araç seti veya PATH içinde bulunamadı.".to_string()),
            issue_count: 0,
        };
    };

    match command.output(&["status"], root) {
        Ok(output) => {
            let text = command_output_text(&output);
            let status = if text.to_ascii_lowercase().contains("not indexed") {
                "not_indexed"
            } else if output.status.success() {
                "ok"
            } else {
                "error"
            };
            ToolStatus {
                tool: "gitnexus".to_string(),
                available: true,
                status: status.to_string(),
                message: Some(compact_tool_message(&text)),
                issue_count: 0,
            }
        }
        Err(e) => ToolStatus {
            tool: "gitnexus".to_string(),
            available: true,
            status: "error".to_string(),
            message: Some(e),
            issue_count: 0,
        },
    }
}

pub fn run_gitnexus_analyze(project_path: &str, force: bool) -> Result<String, String> {
    let root = Path::new(project_path);
    let command = find_tool_command(project_path, "gitnexus")
        .ok_or_else(|| "gitnexus PyShield araç seti veya PATH içinde bulunamadı.".to_string())?;
    let mut args = vec!["analyze", project_path, "--index-only"];
    if force {
        args.push("--force");
    }
    let output = command.output(&args, root)?;
    let text = command_output_text(&output);
    if output.status.success() {
        Ok(text)
    } else {
        Err(compact_tool_message(&text))
    }
}

pub fn run_gitnexus_query(project_path: &str, query: &str, limit: usize) -> Result<String, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("GitNexus sorgusu boş olamaz.".to_string());
    }

    let root = Path::new(project_path);
    let command = find_tool_command(project_path, "gitnexus")
        .ok_or_else(|| "gitnexus PyShield araç seti veya PATH içinde bulunamadı.".to_string())?;
    let repo_selector = gitnexus_repo_selector(project_path);
    let limit_value = limit.clamp(1, 50).to_string();
    let output = command.output(
        &[
            "query",
            "-r",
            repo_selector.as_str(),
            "-l",
            limit_value.as_str(),
            trimmed,
        ],
        root,
    )?;
    let text = command_output_text(&output);
    if output.status.success() {
        Ok(text)
    } else {
        Err(compact_tool_message(&text))
    }
}

fn gitnexus_repo_selector(project_path: &str) -> String {
    Path::new(project_path)
        .file_name()
        .and_then(|part| part.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| project_path.to_string())
}

fn command_output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{}{}", stdout, stderr)
}

fn compact_tool_message(text: &str) -> String {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .collect();
    if lines.is_empty() {
        "Durum çıktısı boş.".to_string()
    } else {
        lines.join(" | ")
    }
}

fn project_python_interpreter(root: &Path) -> Option<String> {
    let candidates = if cfg!(windows) {
        vec![
            root.join(".venv").join("Scripts").join("python.exe"),
            root.join("venv").join("Scripts").join("python.exe"),
            root.join("env").join("Scripts").join("python.exe"),
        ]
    } else {
        vec![
            root.join(".venv").join("bin").join("python"),
            root.join("venv").join("bin").join("python"),
            root.join("env").join("bin").join("python"),
        ]
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(normalize_display_path)
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
        .filter_map(|finding| {
            let file_path = normalize_path(root, &finding.filename);
            if should_suppress_bandit_finding(&finding, &file_path) {
                return None;
            }
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
            Some(issue_from_external_tool(
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
            ))
        })
        .collect())
}

fn should_suppress_bandit_finding(finding: &BanditFinding, file_path: &str) -> bool {
    finding.test_id == "B101" && is_test_path(file_path)
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
    let declared_dependencies = collect_project_dependencies(root);
    Ok(report
        .general_diagnostics
        .into_iter()
        .filter_map(|diagnostic| {
            let file_path = normalize_path(root, &diagnostic.file);
            let rule_id = diagnostic
                .rule
                .clone()
                .unwrap_or_else(|| "PYRIGHT".to_string());
            if should_suppress_pyright_diagnostic(&rule_id, &diagnostic, &declared_dependencies) {
                return None;
            }
            let (category, severity) = pyright_category_and_severity(&rule_id, &diagnostic.severity);
            let line = diagnostic.range.start.line + 1;
            let column = diagnostic.range.start.character + 1;
            let snippet = snippet_for_line(&file_path, line);
            Some(issue_from_external_tool(
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
            ))
        })
        .collect())
}

fn should_suppress_pyright_diagnostic(
    rule_id: &str,
    diagnostic: &PyrightDiagnostic,
    declared_dependencies: &HashSet<String>,
) -> bool {
    matches!(
        rule_id,
        "reportMissingImports" | "reportMissingModuleSource"
    ) && missing_import_name(&diagnostic.message)
        .is_some_and(|import_name| dependency_declares_import(declared_dependencies, &import_name))
}

fn missing_import_name(message: &str) -> Option<String> {
    let start = message.find('"')? + 1;
    let rest = &message[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn dependency_declares_import(declared_dependencies: &HashSet<String>, import_name: &str) -> bool {
    let Some(top_level) = import_name.split('.').next() else {
        return false;
    };
    declared_dependencies.contains(&normalize_dependency_name(top_level))
}

fn collect_project_dependencies(root: &Path) -> HashSet<String> {
    let mut dependencies = HashSet::new();
    collect_pyproject_dependencies(root, &mut dependencies);
    collect_requirements_dependencies(root, &mut dependencies);
    dependencies
}

fn collect_pyproject_dependencies(root: &Path, dependencies: &mut HashSet<String>) {
    let path = root.join("pyproject.toml");
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&content) else {
        return;
    };

    if let Some(project) = value.get("project") {
        collect_dependency_array(project.get("dependencies"), dependencies);
        if let Some(optional) = project
            .get("optional-dependencies")
            .and_then(toml::Value::as_table)
        {
            for deps in optional.values() {
                collect_dependency_array(Some(deps), dependencies);
            }
        }
    }

    collect_dependency_groups(value.get("dependency-groups"), dependencies);

    if let Some(poetry) = value
        .get("tool")
        .and_then(|tool| tool.get("poetry"))
        .and_then(toml::Value::as_table)
    {
        if let Some(deps) = poetry.get("dependencies").and_then(toml::Value::as_table) {
            collect_dependency_table_keys(deps, dependencies);
        }
        if let Some(groups) = poetry.get("group").and_then(toml::Value::as_table) {
            for group in groups.values() {
                if let Some(deps) = group.get("dependencies").and_then(toml::Value::as_table) {
                    collect_dependency_table_keys(deps, dependencies);
                }
            }
        }
    }
}

fn collect_dependency_groups(value: Option<&toml::Value>, dependencies: &mut HashSet<String>) {
    if let Some(groups) = value.and_then(toml::Value::as_table) {
        for group in groups.values() {
            collect_dependency_array(Some(group), dependencies);
        }
    }
}

fn collect_dependency_array(value: Option<&toml::Value>, dependencies: &mut HashSet<String>) {
    if let Some(items) = value.and_then(toml::Value::as_array) {
        for item in items {
            if let Some(raw) = item.as_str() {
                add_dependency_name(raw, dependencies);
            }
        }
    }
}

fn collect_dependency_table_keys(
    table: &toml::map::Map<String, toml::Value>,
    dependencies: &mut HashSet<String>,
) {
    for name in table.keys() {
        if !name.eq_ignore_ascii_case("python") {
            add_dependency_name(name, dependencies);
        }
    }
}

fn collect_requirements_dependencies(root: &Path, dependencies: &mut HashSet<String>) {
    for filename in [
        "requirements.txt",
        "requirements-dev.txt",
        "dev-requirements.txt",
    ] {
        let Ok(content) = std::fs::read_to_string(root.join(filename)) else {
            continue;
        };
        for line in content.lines() {
            let dependency = line.split('#').next().unwrap_or("").trim();
            if dependency.is_empty()
                || dependency.starts_with('-')
                || dependency.starts_with("git+")
                || dependency.starts_with("http:")
                || dependency.starts_with("https:")
            {
                continue;
            }
            add_dependency_name(dependency, dependencies);
        }
    }
}

fn add_dependency_name(raw: &str, dependencies: &mut HashSet<String>) {
    let name: String = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .collect();
    if name.is_empty() {
        return;
    }

    let normalized = normalize_dependency_name(&name);
    dependencies.insert(normalized.clone());
    dependencies.insert(normalized.replace('-', "_"));
    dependencies.insert(normalized.replace('_', "-"));

    for alias in dependency_import_aliases(&normalized) {
        dependencies.insert(alias.to_string());
    }
}

fn normalize_dependency_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

fn dependency_import_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "beautifulsoup4" => &["bs4"],
        "pillow" => &["pil"],
        "pyyaml" => &["yaml"],
        "python_dotenv" => &["dotenv"],
        _ => &[],
    }
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
