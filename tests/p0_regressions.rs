use std::fs;

use ai_code_analyzer::{Severity, analyze_folder, scan_python_files_recursive};

fn fixture_path(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

#[test]
fn clean_alias_and_scope_fixture_has_no_p0_findings() {
    let report = analyze_folder(&fixture_path("p0_clean_alias_scope")).unwrap();

    assert_eq!(report.summary.syntax_errors, 0);
    assert_eq!(report.summary.hallucinations, 0);
    assert_eq!(report.summary.missing_imports, 0);
    assert_eq!(report.summary.undefined_variables, 0);
    assert_eq!(report.summary.security_issues, 0);
    assert_eq!(report.total_errors_found, 0);
}

#[test]
fn dangerous_attribute_calls_and_shell_true_are_reported() {
    let report = analyze_folder(&fixture_path("p0_dangerous")).unwrap();

    assert!(report.summary.security_issues >= 8);
    let issues: Vec<_> = report.files.iter().flat_map(|file| &file.issues).collect();
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains("os.system"))
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains("subprocess.run"))
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains("shell=True"))
    );
    assert!(issues.iter().all(|issue| issue.tool == "pyshield"));
    assert!(issues.iter().all(|issue| !issue.rule_id.is_empty()));
    assert!(issues.iter().any(|issue| issue.severity == Severity::Error));
}

#[test]
fn missing_import_fixture_reports_json_once() {
    let report = analyze_folder(&fixture_path("p0_missing_import")).unwrap();

    assert_eq!(report.summary.missing_imports, 1);
    assert_eq!(report.summary.undefined_variables, 0);
    assert!(
        report.files[0]
            .issues
            .iter()
            .any(|issue| issue.rule_id == "PYSHIELD_IMPORT_001" && issue.message.contains("json"))
    );
}

#[test]
fn syntax_fixture_reports_syntax_error() {
    let report = analyze_folder(&fixture_path("p0_syntax")).unwrap();

    assert!(report.summary.syntax_errors >= 1);
    assert!(
        report.files[0]
            .issues
            .iter()
            .any(|issue| issue.rule_id == "PYSHIELD_SYNTAX_001")
    );
}

#[test]
fn wildcard_fixture_downgrades_to_warnings_without_failing_total() {
    let report = analyze_folder(&fixture_path("p0_wildcard")).unwrap();

    assert_eq!(report.total_errors_found, 0);
    assert!(report.summary.hallucinations > 0 || report.summary.missing_imports > 0);
    assert!(
        report.files[0]
            .issues
            .iter()
            .all(|issue| issue.severity == Severity::Warning)
    );
}

#[test]
fn model_scope_fixture_ignores_class_fields_and_typed_params() {
    let report = analyze_folder(&fixture_path("p0_model_scope")).unwrap();

    assert_eq!(report.summary.missing_imports, 0);
    assert_eq!(report.summary.undefined_variables, 0);
    assert_eq!(report.summary.unused_code, 0);
    assert_eq!(report.total_errors_found, 0);
}

#[test]
fn default_scan_excludes_cache_and_environment_directories() {
    let root = std::env::temp_dir().join(format!("pyshield_exclude_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);

    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".venv")).unwrap();
    fs::create_dir_all(root.join("node_modules")).unwrap();
    fs::write(root.join("src").join("good.py"), "print('ok')\n").unwrap();
    fs::write(root.join(".venv").join("bad.py"), "eval('1')\n").unwrap();
    fs::write(root.join("node_modules").join("bad.py"), "eval('1')\n").unwrap();

    let files = scan_python_files_recursive(root.to_str().unwrap()).unwrap();

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("good.py"));

    fs::remove_dir_all(root).unwrap();
}
