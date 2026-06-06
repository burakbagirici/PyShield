use std::fs;
use std::sync::Mutex;

use ai_code_analyzer::{
    AnalysisOptions, Severity, analyze_folder_with_options, scan_python_files_recursive,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn fixture_path(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn analyze_fixture(name: &str) -> ai_code_analyzer::AnalysisReport {
    let mut options = AnalysisOptions::default();
    options.enable_external_tools = false;
    analyze_folder_with_options(&fixture_path(name), &options).unwrap()
}

#[test]
fn clean_alias_and_scope_fixture_has_no_p0_findings() {
    let report = analyze_fixture("p0_clean_alias_scope");

    assert_eq!(report.summary.syntax_errors, 0);
    assert_eq!(report.summary.hallucinations, 0);
    assert_eq!(report.summary.missing_imports, 0);
    assert_eq!(report.summary.undefined_variables, 0);
    assert_eq!(report.summary.security_issues, 0);
    assert_eq!(report.total_errors_found, 0);
}

#[test]
fn dangerous_attribute_calls_and_shell_true_are_reported() {
    let report = analyze_fixture("p0_dangerous");

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
    let report = analyze_fixture("p0_missing_import");

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
    let report = analyze_fixture("p0_syntax");

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
    let report = analyze_fixture("p0_wildcard");

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
    let report = analyze_fixture("p0_model_scope");

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
    fs::create_dir_all(root.join(".gitnexus")).unwrap();
    fs::create_dir_all(root.join(".pyshield-tools")).unwrap();
    fs::create_dir_all(root.join(".venv")).unwrap();
    fs::create_dir_all(root.join("node_modules")).unwrap();
    fs::write(root.join("src").join("good.py"), "print('ok')\n").unwrap();
    fs::write(root.join(".gitnexus").join("bad.py"), "eval('1')\n").unwrap();
    fs::write(root.join(".pyshield-tools").join("bad.py"), "eval('1')\n").unwrap();
    fs::write(root.join(".venv").join("bad.py"), "eval('1')\n").unwrap();
    fs::write(root.join("node_modules").join("bad.py"), "eval('1')\n").unwrap();

    let files = scan_python_files_recursive(root.to_str().unwrap()).unwrap();

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("good.py"));

    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn external_adapters_merge_managed_tool_outputs() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "pyshield_adapter_target_test_{}",
        std::process::id()
    ));
    let tools_root = std::env::temp_dir().join(format!(
        "pyshield_adapter_tools_test_{}",
        std::process::id()
    ));
    let python_scripts = tools_root.join("python").join("Scripts");
    let node_bin = tools_root.join("node").join("node_modules").join(".bin");
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&tools_root);

    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(&python_scripts).unwrap();
    fs::create_dir_all(&node_bin).unwrap();
    fs::write(root.join("sample.py"), "print('ok')\n").unwrap();
    fs::write(root.join("tests").join("test_sample.py"), "assert True\n").unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\ndependencies = [\"asyncpg>=0.29.0\"]\n",
    )
    .unwrap();
    fs::write(
        python_scripts.join("ruff.cmd"),
        "@echo off\r\nif \"%1\"==\"--version\" (echo ruff 0.0.0& exit /b 0)\r\necho [{\"filename\":\"sample.py\",\"code\":\"F821\",\"message\":\"Undefined name `ok`\",\"location\":{\"row\":1,\"column\":7},\"fix\":null}]\r\n",
    )
    .unwrap();
    fs::write(
        python_scripts.join("bandit.cmd"),
        "@echo off\r\nif \"%1\"==\"--version\" (echo bandit 0.0.0& exit /b 0)\r\necho {\"results\":[{\"filename\":\"sample.py\",\"line_number\":1,\"issue_text\":\"Possible shell injection\",\"issue_severity\":\"HIGH\",\"test_id\":\"B602\",\"code\":\"print('ok')\"},{\"filename\":\"tests/test_sample.py\",\"line_number\":1,\"issue_text\":\"Use of assert detected\",\"issue_severity\":\"LOW\",\"test_id\":\"B101\",\"code\":\"assert True\"}]}\r\n",
    )
    .unwrap();
    fs::write(
        node_bin.join("pyright.cmd"),
        "@echo off\r\nif \"%1\"==\"--version\" (echo pyright 0.0.0& exit /b 0)\r\necho {\"generalDiagnostics\":[{\"file\":\"sample.py\",\"severity\":\"error\",\"message\":\"Import \\\"asyncpg\\\" could not be resolved\",\"rule\":\"reportMissingImports\",\"range\":{\"start\":{\"line\":0,\"character\":0}}},{\"file\":\"sample.py\",\"severity\":\"error\",\"message\":\"Undefined variable\",\"rule\":\"reportUndefinedVariable\",\"range\":{\"start\":{\"line\":0,\"character\":0}}}]}\r\n",
    )
    .unwrap();
    fs::write(
        node_bin.join("gitnexus.cmd"),
        "@echo off\r\nif \"%1\"==\"--version\" (echo 1.6.5& exit /b 0)\r\nif \"%1\"==\"status\" (echo Status: up-to-date& exit /b 0)\r\necho ok\r\n",
    )
    .unwrap();

    let previous_tools_dir = std::env::var_os("PYSHIELD_TOOLS_DIR");
    unsafe {
        std::env::set_var("PYSHIELD_TOOLS_DIR", &tools_root);
    }

    let report = ai_code_analyzer::analyze_folder(root.to_str().unwrap()).unwrap();
    let mut ruff_only_options = AnalysisOptions::default();
    ruff_only_options.enable_native_analysis = false;
    ruff_only_options.enable_external_tools = true;
    ruff_only_options.enabled_external_tools = vec!["ruff".to_string()];
    let ruff_only_report =
        analyze_folder_with_options(root.to_str().unwrap(), &ruff_only_options).unwrap();

    unsafe {
        if let Some(previous_tools_dir) = previous_tools_dir {
            std::env::set_var("PYSHIELD_TOOLS_DIR", previous_tools_dir);
        } else {
            std::env::remove_var("PYSHIELD_TOOLS_DIR");
        }
    }

    let issues: Vec<_> = report.files.iter().flat_map(|file| &file.issues).collect();

    assert_eq!(report.tool_statuses.len(), 4);
    assert!(
        report
            .tool_statuses
            .iter()
            .all(|status| status.available && status.status == "ok")
    );
    assert!(issues.iter().any(|issue| issue.tool == "ruff"));
    assert!(issues.iter().any(|issue| issue.tool == "bandit"));
    assert!(issues.iter().any(|issue| issue.tool == "pyright"));
    assert!(
        report
            .tool_statuses
            .iter()
            .any(|status| status.tool == "gitnexus" && status.status == "ok")
    );
    assert_eq!(ruff_only_report.tool_statuses.len(), 1);
    assert_eq!(ruff_only_report.tool_statuses[0].tool, "ruff");
    assert!(
        ruff_only_report
            .files
            .iter()
            .flat_map(|file| &file.issues)
            .all(|issue| issue.tool == "ruff")
    );
    assert!(
        !issues
            .iter()
            .any(|issue| issue.tool == "bandit" && issue.rule_id == "B101")
    );
    assert!(
        !issues
            .iter()
            .any(|issue| issue.tool == "pyright" && issue.rule_id == "reportMissingImports")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.tool == "pyright" && issue.rule_id == "reportUndefinedVariable")
    );

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(tools_root).unwrap();
}
