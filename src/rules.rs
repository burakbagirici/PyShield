use std::collections::HashSet;
use std::sync::OnceLock;
use tree_sitter::{Node, Query, QueryCursor};
// HATA DÜZELTMESİ: tree-sitter üzerinden değil, doğrudan kendi kütüphanesinden çağırıyoruz
use streaming_iterator::StreamingIterator;

// ===== STATIK QUERY OPTIMIZASYONU (Zero-Allocation Queries) =====
// Her query bir kere derlenir ve program ömrü boyunca saklanır

static QUERY_FUNC_CALLS: OnceLock<Query> = OnceLock::new();
static QUERY_FUNC_DEFS: OnceLock<Query> = OnceLock::new();
static QUERY_CLASS_DEFS: OnceLock<Query> = OnceLock::new();
static QUERY_PARAMS: OnceLock<Query> = OnceLock::new();
static QUERY_ASSIGNMENTS: OnceLock<Query> = OnceLock::new();
static QUERY_FOR_LOOPS: OnceLock<Query> = OnceLock::new();
static QUERY_IDENTIFIERS: OnceLock<Query> = OnceLock::new();
static QUERY_ATTRIBUTES: OnceLock<Query> = OnceLock::new();
static QUERY_ASSIGN_STRINGS: OnceLock<Query> = OnceLock::new();
static QUERY_WHILE_LOOPS: OnceLock<Query> = OnceLock::new();
static QUERY_DANGEROUS_EXECUTIONS: OnceLock<Query> = OnceLock::new();
static BUILTIN_FUNCTIONS_SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
static KEYWORD_ARGUMENT_NAMES_SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
static PYTHON_KEYWORDS_SET: OnceLock<HashSet<&'static str>> = OnceLock::new();

/// Python yerleşik (built-in) fonksiyonları, istisnalar ve özel nitelikleri
/// Tanımsız değişken analizinden muaf tutulacak identifiers
pub const BUILTIN_FUNCTIONS: &[&str] = &[
    // Built-in fonksiyonlar
    "print",
    "len",
    "range",
    "input",
    "type",
    "str",
    "int",
    "float",
    "list",
    "dict",
    "set",
    "tuple",
    "bool",
    "isinstance",
    "hasattr",
    "getattr",
    "setattr",
    "open",
    "super",
    "enumerate",
    "zip",
    "map",
    "filter",
    "sorted",
    "reversed",
    "abs",
    "max",
    "min",
    "sum",
    "any",
    "all",
    "id",
    "hash",
    "repr",
    "hex",
    "oct",
    "bin",
    "chr",
    "ord",
    "round",
    "format",
    "iter",
    "next",
    "vars",
    "dir",
    "help",
    "exit",
    "quit",
    "exec",
    "eval",
    "compile",
    "globals",
    "locals",
    "staticmethod",
    "classmethod",
    "property",
    "object",
    "callable",
    "complex",
    "slice",
    "divmod",
    "pow",
    "bytes",
    "bytearray",
    "memoryview",
    "frozenset",
    "ascii",
    // Built-in istisnalar ve hata sınıfları
    "Exception",
    "BaseException",
    "SystemExit",
    "KeyboardInterrupt",
    "GeneratorExit",
    "StopIteration",
    "StopAsyncIteration",
    "ArithmeticError",
    "FloatingPointError",
    "OverflowError",
    "ZeroDivisionError",
    "AssertionError",
    "AttributeError",
    "BufferError",
    "EOFError",
    "ImportError",
    "ModuleNotFoundError",
    "LookupError",
    "IndexError",
    "KeyError",
    "MemoryError",
    "NameError",
    "UnboundLocalError",
    "OSError",
    "BlockingIOError",
    "ChildProcessError",
    "ConnectionError",
    "BrokenPipeError",
    "ConnectionAbortedError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "FileExistsError",
    "FileNotFoundError",
    "InterruptedError",
    "IsADirectoryError",
    "NotADirectoryError",
    "PermissionError",
    "ProcessLookupError",
    "TimeoutError",
    "ReferenceError",
    "RuntimeError",
    "NotImplementedError",
    "RecursionError",
    "SyntaxError",
    "IndentationError",
    "TabError",
    "SystemError",
    "TypeError",
    "ValueError",
    "UnicodeError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeTranslateError",
    "Warning",
    "DeprecationWarning",
    "PendingDeprecationWarning",
    "RuntimeWarning",
    "SyntaxWarning",
    "UserWarning",
    "FutureWarning",
    "ImportWarning",
    "UnicodeWarning",
    "BytesWarning",
    "ResourceWarning",
    // Özel/magic değişkenler
    "__name__",
    "__file__",
    "__doc__",
    "__package__",
    "__loader__",
    "__spec__",
    "__build_class__",
    "__import__",
    "__cached__",
    "__path__",
    "__module__",
    "__dict__",
    "__slots__",
    "__weakref__",
    "__class__",
    "__bases__",
    "__mro__",
    "__qualname__",
    "__annotations__",
    // Özel scope değişkenleri
    "self",
    "cls",
];

/// Keyword argument parametreleri (argparse ve diğer fonksiyonlarda sık rastlanır)
/// Bu isimler fonksiyon çağrıları içinde tanımsız değişken olarak işaretlenmemeli
pub const KEYWORD_ARGUMENT_NAMES: &[&str] = &[
    "description",
    "metavar",
    "dest",
    "action",
    "required",
    "default",
    "type",
    "choices",
    "nargs",
    "const",
    "help",
    "formatter_class",
    "prefix_chars",
    "fromfile_prefix_chars",
    "argument_default",
    "conflict_handler",
    "add_help",
    "name",
    "version",
    "kwargs",
];

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportSymbol<'a> {
    pub module: &'a str,
    pub imported_name: &'a str,
    pub local_name: &'a str,
    pub is_wildcard: bool,
    pub line: usize,
}

fn builtin_functions_set() -> &'static HashSet<&'static str> {
    BUILTIN_FUNCTIONS_SET.get_or_init(|| BUILTIN_FUNCTIONS.iter().copied().collect())
}

fn keyword_argument_names_set() -> &'static HashSet<&'static str> {
    KEYWORD_ARGUMENT_NAMES_SET.get_or_init(|| KEYWORD_ARGUMENT_NAMES.iter().copied().collect())
}

fn python_keywords_set() -> &'static HashSet<&'static str> {
    PYTHON_KEYWORDS_SET.get_or_init(|| PYTHON_KEYWORDS.iter().copied().collect())
}

/// YARDIMCI FONKSIYON (Sorgu Optimizasyonu):
/// Kodda çağrılan tüm fonksiyonları tek bir seferde bulur.
/// Sorgu derlemesi sadece bir kere yapılır (zero-allocation).
pub fn collect_all_function_calls<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
) -> Vec<(Node<'a>, &'a str)> {
    let query = QUERY_FUNC_CALLS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(call function: (identifier) @func_name)",
        )
        .expect("Fonksiyon çağrısı sorgusu derlenirken hata oluştu!")
    });

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(query, root_node, source_code.as_bytes());

    let mut all_calls = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(func_name) = capture.node.utf8_text(source_code.as_bytes()) {
                all_calls.push((capture.node, func_name));
            }
        }
    }

    all_calls
}

/// AKILLI KURAL 2:
/// Kodda tanımlanan tüm fonksiyonları bulur ve isimlerini (borçlanmış) döner.
pub fn find_function_definitions<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let query = QUERY_FUNC_DEFS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(function_definition name: (identifier) @def_name)",
        )
        .expect("Fonksiyon tanımı sorgusu derlenirken hata oluştu!")
    });

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(query, root_node, source_code.as_bytes());

    let mut defined_functions = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(func_name) = capture.node.utf8_text(source_code.as_bytes()) {
                defined_functions.push(func_name);
            }
        }
    }

    defined_functions
}

pub fn find_class_definitions<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let query = QUERY_CLASS_DEFS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(class_definition name: (identifier) @class_name)",
        )
        .expect("Class tanımı sorgusu derlenirken hata oluştu!")
    });

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(query, root_node, source_code.as_bytes());

    let mut defined_classes = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(class_name) = capture.node.utf8_text(source_code.as_bytes()) {
                defined_classes.push(class_name);
            }
        }
    }

    defined_classes
}

/// TEMEL KURAL:
/// Sözdizimi hatalarını bulur.
pub fn check_for_errors<'a>(node: Node<'a>, errors: &mut Vec<Node<'a>>) {
    if node.is_error() {
        errors.push(node);
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            check_for_errors(child, errors);
        }
    }
}

fn syntax_error_kind_hint(kind: &str) -> Option<&'static str> {
    match kind {
        "integer" | "float" => Some("sayı ifadesi"),
        "string" | "concatenated_string" => Some("metin ifadesi"),
        "identifier" => Some("tanımlayıcı"),
        "(" | ")" => Some("parantez"),
        "[" | "]" => Some("köşeli parantez"),
        "{" | "}" => Some("süslü parantez"),
        "comment" => Some("yorum"),
        "def" | "class" | "if" | "for" | "while" | "return" => Some("anahtar sözcük"),
        _ => None,
    }
}

/// Tree-sitter ERROR düğümü için Türkçe, kullanıcı dostu mesaj.
pub fn format_syntax_error(node: &Node) -> String {
    let line = node.start_position().row + 1;
    let col = node.start_position().column + 1;
    let base = format!(
        "Satır {}, Sütun {}: Yapay zeka bu bölgede geçersiz veya eksik bir sözdizimi üretti.",
        line, col
    );
    match syntax_error_kind_hint(node.kind()) {
        Some(hint) => format!("{} ({})", base, hint),
        None => base,
    }
}

fn collect_nodes_by_kind<'a>(node: Node<'a>, kind: &str, out: &mut Vec<Node<'a>>) {
    if node.kind() == kind {
        out.push(node);
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_nodes_by_kind(child, kind, out);
        }
    }
}

fn strip_comment(text: &str) -> &str {
    text.split('#').next().unwrap_or(text).trim()
}

fn split_import_items(text: &str) -> Vec<&str> {
    text.trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .map(strip_comment)
        .filter(|item| !item.is_empty())
        .collect()
}

fn split_alias(text: &str) -> (&str, &str) {
    let cleaned = strip_comment(text);
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() == 3 && parts[1].eq_ignore_ascii_case("as") {
        (parts[0].trim(), parts[2].trim())
    } else {
        (cleaned, cleaned)
    }
}

fn first_dotted_segment(text: &str) -> &str {
    text.split('.').next().unwrap_or(text).trim()
}

fn last_dotted_segment(text: &str) -> &str {
    text.rsplit('.').next().unwrap_or(text).trim()
}

fn parse_import_statement<'a>(text: &'a str, line: usize, imports: &mut Vec<ImportSymbol<'a>>) {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("import") else {
        return;
    };

    for item in split_import_items(rest) {
        let (module_name, alias_or_name) = split_alias(item);
        let local_name = if alias_or_name == module_name {
            first_dotted_segment(module_name)
        } else {
            alias_or_name
        };

        if !module_name.is_empty() && !local_name.is_empty() {
            imports.push(ImportSymbol {
                module: module_name,
                imported_name: module_name,
                local_name,
                is_wildcard: false,
                line,
            });
        }
    }
}

fn parse_from_import_statement<'a>(
    text: &'a str,
    line: usize,
    imports: &mut Vec<ImportSymbol<'a>>,
) {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("from") else {
        return;
    };

    let rest = rest.trim_start();
    let Some(import_idx) = rest.find(" import") else {
        return;
    };

    let module_name = rest[..import_idx].trim();
    let imported_part = rest[import_idx + " import".len()..].trim();
    if imported_part == "*" {
        imports.push(ImportSymbol {
            module: module_name,
            imported_name: "*",
            local_name: "*",
            is_wildcard: true,
            line,
        });
        return;
    }

    for item in split_import_items(imported_part) {
        if item == "*" {
            imports.push(ImportSymbol {
                module: module_name,
                imported_name: "*",
                local_name: "*",
                is_wildcard: true,
                line,
            });
            continue;
        }

        let (imported_name, alias_or_name) = split_alias(item);
        let local_name = if alias_or_name == imported_name {
            last_dotted_segment(imported_name)
        } else {
            alias_or_name
        };

        if !imported_name.is_empty() && !local_name.is_empty() {
            imports.push(ImportSymbol {
                module: module_name,
                imported_name,
                local_name,
                is_wildcard: false,
                line,
            });
        }
    }
}

pub fn collect_import_symbols<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
) -> Vec<ImportSymbol<'a>> {
    let mut import_nodes = Vec::new();
    collect_nodes_by_kind(root_node, "import_statement", &mut import_nodes);
    collect_nodes_by_kind(root_node, "import_from_statement", &mut import_nodes);

    import_nodes.sort_by_key(|node| node.start_byte());

    let mut imports = Vec::new();
    for node in import_nodes {
        if let Ok(text) = node.utf8_text(source_code.as_bytes()) {
            let line = node.start_position().row + 1;
            match node.kind() {
                "import_statement" => parse_import_statement(text, line, &mut imports),
                "import_from_statement" => parse_from_import_statement(text, line, &mut imports),
                _ => {}
            }
        }
    }

    imports
}

/// AKILLI KURAL 3:
/// Kodda import edilen tüm kütüphaneleri bulur.
pub fn find_all_imports<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    collect_import_symbols(root_node, source_code)
        .into_iter()
        .filter(|symbol| !symbol.is_wildcard)
        .filter_map(|symbol| {
            if seen.insert(symbol.local_name) {
                Some(symbol.local_name)
            } else {
                None
            }
        })
        .collect()
}

fn collect_binding_identifiers<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    match node.kind() {
        "identifier" => out.push(node),
        "attribute" | "subscript" => return,
        _ => {
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    collect_binding_identifiers(child, out);
                }
            }
        }
    }
}

fn collect_parameter_identifiers<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    if node.kind() == "identifier" {
        out.push(node);
        return;
    }

    if let Some(name_node) = node.child_by_field_name("name") {
        collect_binding_identifiers(name_node, out);
        return;
    }

    if let Some(pattern_node) = node.child_by_field_name("pattern") {
        collect_binding_identifiers(pattern_node, out);
        return;
    }

    if matches!(
        node.kind(),
        "typed_parameter"
            | "default_parameter"
            | "typed_default_parameter"
            | "list_splat_pattern"
            | "dictionary_splat_pattern"
            | "keyword_separator"
    ) {
        if let Some(first_named_child) = node.named_child(0) {
            collect_binding_identifiers(first_named_child, out);
        }
        return;
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_parameter_identifiers(child, out);
        }
    }
}

fn collect_with_aliases<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let mut with_nodes = Vec::new();
    collect_nodes_by_kind(root_node, "with_statement", &mut with_nodes);

    let mut aliases = Vec::new();
    for node in with_nodes {
        let Ok(text) = node.utf8_text(source_code.as_bytes()) else {
            continue;
        };
        let Some(header) = text.split(':').next() else {
            continue;
        };
        let Some(rest) = header.trim_start().strip_prefix("with") else {
            continue;
        };

        for item in split_import_items(rest) {
            let parts: Vec<&str> = item.split_whitespace().collect();
            if parts.len() >= 3 {
                for idx in 0..parts.len().saturating_sub(1) {
                    if parts[idx].eq_ignore_ascii_case("as") {
                        let alias = parts[idx + 1]
                            .trim()
                            .trim_start_matches('(')
                            .trim_end_matches(')')
                            .trim_end_matches(':');
                        if !alias.is_empty() {
                            aliases.push(alias);
                        }
                    }
                }
            }
        }
    }

    aliases
}

fn collect_binding_names_from_kind<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
    kind: &str,
) -> Vec<&'a str> {
    let mut nodes = Vec::new();
    collect_nodes_by_kind(root_node, kind, &mut nodes);

    let mut names = Vec::new();
    for node in nodes {
        let mut binding_nodes = Vec::new();
        collect_binding_identifiers(node, &mut binding_nodes);
        for binding_node in binding_nodes {
            if let Ok(name) = binding_node.utf8_text(source_code.as_bytes()) {
                names.push(name);
            }
        }
    }

    names
}

fn collect_first_named_child_binding_names_from_kind<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
    kind: &str,
) -> Vec<&'a str> {
    let mut nodes = Vec::new();
    collect_nodes_by_kind(root_node, kind, &mut nodes);

    let mut names = Vec::new();
    for node in nodes {
        let Some(target_node) = node.named_child(0) else {
            continue;
        };

        let mut binding_nodes = Vec::new();
        collect_binding_identifiers(target_node, &mut binding_nodes);
        for binding_node in binding_nodes {
            if let Ok(name) = binding_node.utf8_text(source_code.as_bytes()) {
                names.push(name);
            }
        }
    }

    names
}

fn has_ancestor_kind(node: Node, kinds: &[&str]) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if kinds.contains(&parent.kind()) {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn is_class_scope_binding(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "function_definition" | "lambda" => return false,
            "class_definition" => return true,
            _ => current = parent.parent(),
        }
    }
    false
}

/// AKILLI KURAL 3 (DEVAM):
/// Kodda tanımlanan tüm değişkenleri bulur (parametreler, assignmentlar, loop vars, imports).
pub fn find_defined_variables<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let mut defined_vars = Vec::new();

    for import in collect_import_symbols(root_node, source_code) {
        if !import.is_wildcard {
            defined_vars.push(import.local_name);
        }
    }

    for alias in collect_with_aliases(root_node, source_code) {
        defined_vars.push(alias);
    }

    for alias in collect_binding_names_from_kind(root_node, source_code, "as_pattern_target") {
        defined_vars.push(alias);
    }

    for lambda_param in collect_binding_names_from_kind(root_node, source_code, "lambda_parameters")
    {
        defined_vars.push(lambda_param);
    }

    for comprehension_target in
        collect_first_named_child_binding_names_from_kind(root_node, source_code, "for_in_clause")
    {
        defined_vars.push(comprehension_target);
    }

    for class_name in find_class_definitions(root_node, source_code) {
        defined_vars.push(class_name);
    }

    let id_query = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(&tree_sitter_python::LANGUAGE.into(), "(identifier) @id")
            .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });

    // Fonksiyon parametreleri: def func(x, y, z)
    let param_query = QUERY_PARAMS.get_or_init(|| {
        Query::new(&tree_sitter_python::LANGUAGE.into(), "(parameters) @params")
            .expect("Params sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(param_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let mut param_nodes = Vec::new();
            collect_parameter_identifiers(capture.node, &mut param_nodes);
            for param_node in param_nodes {
                if let Ok(param_name) = param_node.utf8_text(source_code.as_bytes()) {
                    defined_vars.push(param_name);
                }
            }
        }
    }

    // Assignment'lar: x = 10 (BUT NOT keyword arguments: func(x=10))
    let assign_query = QUERY_ASSIGNMENTS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(assignment left: (_) @lhs)",
        )
        .expect("Assignments sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(assign_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let mut binding_nodes = Vec::new();
            collect_binding_identifiers(capture.node, &mut binding_nodes);
            for binding_node in binding_nodes {
                if let Ok(var_name) = binding_node.utf8_text(source_code.as_bytes()) {
                    if !is_keyword_argument(binding_node) {
                        defined_vars.push(var_name);
                    }
                }
            }
        }
    }

    // For loop değişkenleri: for i in range(10)
    let for_query = QUERY_FOR_LOOPS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(for_statement left: (_) @loop_var)",
        )
        .expect("For loops sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(for_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let mut binding_nodes = Vec::new();
            collect_binding_identifiers(capture.node, &mut binding_nodes);
            for binding_node in binding_nodes {
                if let Ok(loop_var) = binding_node.utf8_text(source_code.as_bytes()) {
                    defined_vars.push(loop_var);
                }
            }
        }
    }

    // Exception handler parametreleri: except ValueError as e:
    // Tüm identifier'ları yakala ve except handler kontrol et
    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(id_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source_code.as_bytes()) {
                // Parent check: except clause altında mı?
                let mut parent_opt = capture.node.parent();

                for _ in 0..5 {
                    if let Some(parent) = parent_opt {
                        if parent.kind() == "except_clause" {
                            // Exception handler içindeki identifier tanımlı
                            defined_vars.push(name);
                            break;
                        }
                        parent_opt = parent.parent();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    defined_vars
}

fn is_inside_comprehension(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        let kind = n.kind();
        if kind == "list_comprehension"
            || kind == "set_comprehension"
            || kind == "dictionary_comprehension"
            || kind == "generator_expression"
        {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Bir identifier'ın keyword argument içinde olup olmadığını kontrol eder.
/// Örn: func(param_name="value") içindeki param_name keyword_argument node'u içinde olacak
fn is_keyword_argument(node: Node) -> bool {
    fn is_keyword_argument_parent_kind(kind: &str) -> bool {
        kind == "keyword_argument"
            || kind == "named_argument"
            || kind == "keyword_parameter"
            || kind == "default_parameter"
            || (kind.contains("keyword") && kind.contains("argument"))
    }

    fn is_left_side_of_argument(parent: Node, node: Node) -> bool {
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return true;
            }
        }

        if let Some(first_named_child) = parent.named_child(0) {
            if first_named_child.id() == node.id() {
                return true;
            }
        }

        if let Some(first_child) = parent.child(0) {
            if first_child.id() == node.id() {
                return true;
            }
        }

        false
    }

    let mut current = node.parent();
    while let Some(n) = current {
        let kind = n.kind();
        if is_keyword_argument_parent_kind(kind) {
            return is_left_side_of_argument(n, node);
        }

        if matches!(
            kind,
            "call" | "argument_list" | "arguments" | "parenthesized_expression"
        ) {
            current = n.parent();
            continue;
        }

        current = n.parent();
    }
    false
}

/// Bir node'un decorator context'inde olup olmadığını kontrol eder.
fn is_in_decorator(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "decorator" {
            return true;
        }
        // Sadece decorated_definition'a kadar çık
        if n.kind() == "decorated_definition" {
            return false;
        }
        current = n.parent();
    }
    false
}

/// Wildcard import (`from x import *`) olup olmadığını kontrol eder
pub fn has_wildcard_import(root_node: Node) -> bool {
    fn search_wildcard(node: Node) -> bool {
        if node.kind() == "import_from_statement" {
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i) {
                    if matches!(child.kind(), "*" | "import_star" | "wildcard_import") {
                        return true;
                    }
                }
            }
        }

        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i) {
                if search_wildcard(child) {
                    return true;
                }
            }
        }
        false
    }
    search_wildcard(root_node)
}

/// AKILLI KURAL 3 (İŞLEV):
/// Kodda kullanılan tanımlanmamış değişkenleri bulur.
/// Production-ready false positive filtreleme sistemi:
/// - Built-in özel değişkenler, fonksiyonlar, istisnalar
/// - Keyword argument'lar (func(param=value) içinde param)
/// - Decorator context'i
/// - self/cls scope yönetimi  
pub fn find_undefined_variables<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
    hallucinated: &[&'a str],
    missing_imports: &[&'a str],
) -> Vec<(Node<'a>, &'a str)> {
    let defined_vars = find_defined_variables(root_node, source_code);
    let defined: HashSet<&str> = defined_vars.into_iter().collect();
    let builtin_functions_lookup = builtin_functions_set();
    let keyword_argument_names_lookup = keyword_argument_names_set();
    let python_keywords_lookup = python_keywords_set();

    // Function definition names'ı topla (bunlar değişken değil)
    let func_names_vec = find_function_definitions(root_node, source_code);
    let func_names: HashSet<&str> = func_names_vec.into_iter().collect();
    let hallucinated_names: HashSet<&str> = hallucinated.iter().copied().collect();
    let missing_import_names: HashSet<&str> = missing_imports.iter().copied().collect();

    // Tüm identifier usageları: herhangi bir identifier context'te
    let usage_query = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(&tree_sitter_python::LANGUAGE.into(), "(identifier) @id")
            .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });

    let mut undefined_vars = Vec::new();

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(usage_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(var_name) = capture.node.utf8_text(source_code.as_bytes()) {
                // 1. Comprehension içindeyse atla
                if is_inside_comprehension(capture.node) {
                    continue;
                }

                // 2. Decorator context'inde ise atla
                if is_in_decorator(capture.node) {
                    continue;
                }

                // 3. Keyword argument olup olmadığını kontrol et
                if is_keyword_argument(capture.node) {
                    continue;
                }

                // Node'un parent'ını kontrol et (import, function def, attribute vb)
                let parent = capture.node.parent();

                // 4. İlgisiz parent types'ını skip et
                if let Some(p) = parent {
                    let parent_kind = p.kind();

                    // Import statement içindeki identifiers skip
                    if parent_kind == "import_statement"
                        || parent_kind == "import_from_statement"
                        || parent_kind == "dotted_name"
                    {
                        continue;
                    }

                    // Function definition adı skip
                    if parent_kind == "function_definition" || parent_kind == "class_definition" {
                        continue;
                    }

                    // Attribute'un attribute adı kısmı skip (sağ taraf)
                    if parent_kind == "attribute" {
                        // Eğer parent'ın attribute field'i bu node'u contain ediyorsa skip
                        if let Some(attr_node) = p.child_by_field_name("attribute") {
                            if attr_node.id() == capture.node.id() {
                                continue;
                            }
                        }
                    }
                }

                // 5. Eğer tanımlanmamışsa ve built-in değilse, undefined
                if !defined.contains(var_name)
                    && !builtin_functions_lookup.contains(var_name)
                    && !keyword_argument_names_lookup.contains(var_name)
                    && !func_names.contains(var_name)
                    && !python_keywords_lookup.contains(var_name)
                    && !hallucinated_names.contains(var_name)
                    && !missing_import_names.contains(var_name)
                {
                    undefined_vars.push((capture.node, var_name));
                }
            }
        }
    }

    undefined_vars
}

/// KURAL 5: Kullanılmayan Kod Tespiti (Unused Code Detector)
/// Tanımlanmış ama hiç kullanılmayan import'ları ve değişkenleri bulur.
/// Dönen tuple: (ad &str, satır numarası usize, kategori &str)
pub fn find_unused_code<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
) -> Vec<(&'a str, usize, &'static str)> {
    let mut unused = Vec::new();

    // --- Kullanılan tüm identifier'ları topla ---
    // (Sadece "kullanım" context'lerinden: atama solu değil, import tanımı değil)
    let mut used_names: Vec<&'a str> = Vec::new();

    let id_query = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(&tree_sitter_python::LANGUAGE.into(), "(identifier) @id")
            .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(id_query, root_node, source_code.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source_code.as_bytes()) {
                if has_ancestor_kind(capture.node, &["import_statement", "import_from_statement"]) {
                    continue;
                }

                let parent = capture.node.parent();
                if let Some(p) = parent {
                    let pk = p.kind();
                    // Import tanım yerleri ve assignment sol tarafı
                    // "kullanım" sayılmaz
                    if pk == "import_statement"
                        || pk == "import_from_statement"
                        || pk == "dotted_name"
                    {
                        continue;
                    }
                    // Assignment'ın sol tarafındaki identifier kullanım değil
                    if pk == "assignment" {
                        if let Some(lhs) = p.child_by_field_name("left") {
                            if lhs.id() == capture.node.id() {
                                continue;
                            }
                        }
                    }
                }
                used_names.push(name);
            }
        }
    }

    // --- 1. Kullanılmayan import'lar ---
    for import in collect_import_symbols(root_node, source_code) {
        if import.is_wildcard {
            continue;
        }
        if !used_names.contains(&import.local_name) {
            unused.push((import.local_name, import.line, "import"));
        }
    }

    // --- 2. Kullanılmayan değişkenler ---
    // x = 5 -> x hiç okunmuyor mu?
    let assign_query = QUERY_ASSIGNMENTS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(assignment left: (identifier) @lhs)",
        )
        .expect("Assignments sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(assign_query, root_node, source_code.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let mut binding_nodes = Vec::new();
            collect_binding_identifiers(capture.node, &mut binding_nodes);
            for binding_node in binding_nodes {
                if is_class_scope_binding(binding_node) {
                    continue;
                }
                if let Ok(var_name) = binding_node.utf8_text(source_code.as_bytes()) {
                    let row = binding_node.start_position().row + 1;
                    // "_" ile başlayanlar (konvansiyonel olarak kasıtlı unused)
                    if var_name.starts_with('_') {
                        continue;
                    }
                    if !used_names.contains(&var_name) {
                        unused.push((var_name, row, "değişken"));
                    }
                }
            }
        }
    }

    unused
}

/// AKILLI KURAL 4:
/// Kodda noktalı notasyon ile kullanılan ama import edilmemiş kütüphaneleri bulur.
/// Örn: math.sqrt() veya os.path - math ve os import edilmemişse hata.
/// Wildcard import yönetimi: from x import * varsa, false positive'leri engelle
/// Scope muafiyeti: self.attr ve cls.attr'leri skip et (dinamik attribute)
pub fn find_missing_imports<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
    imports: &[&'a str],
) -> Vec<(Node<'a>, &'a str)> {
    let defined_vars = find_defined_variables(root_node, source_code);
    let defined: HashSet<&str> = defined_vars.into_iter().collect();
    let builtin_functions_lookup = builtin_functions_set();

    // Attribute access: obj.attr pattern'ı
    let attr_query = QUERY_ATTRIBUTES.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(attribute object: (identifier) @obj)",
        )
        .expect("Attributes sorgusu derlenirken hata oluştu!")
    });

    let mut missing = Vec::new();

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(attr_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(lib_name) = capture.node.utf8_text(source_code.as_bytes()) {
                // self.attribute veya cls.attribute'ı skip et (dinamik scope)
                if lib_name == "self" || lib_name == "cls" {
                    continue;
                }

                if defined.contains(lib_name) || builtin_functions_lookup.contains(lib_name) {
                    continue;
                }

                // Eğer library'nin adı import edilmemişse, bu eksik import.
                // Wildcard import durumunda severity kararını raporlama katmanı verir.
                if !imports.contains(&lib_name) {
                    missing.push((capture.node, lib_name));
                }
            }
        }
    }

    missing
}

/// KURAL 6: Güvenlik Açığı Tespiti (Secret & API Key Detection)
/// Kod içindeki string literal'leri tarar, bilinen API key prefix'lerini ve
/// hassas değişken adlarını (password, secret, api_key, token) tespit eder.
/// Dönen tuple: (değişken_adı &str, satır usize, string_içeriği &str)
pub fn find_hardcoded_secrets<'a, 'b>(
    root_node: Node<'a>,
    source_code: &'b str,
) -> Vec<(&'b str, usize, &'b str)> {
    let mut secrets = Vec::new();

    // Bilinen API key prefix'leri
    const KEY_PREFIXES: &[&str] = &["sk-", "AIza", "amzn.mws", "AKIA", "ghp_", "glpat-"];
    // Hassas değişken adı kalıpları
    const SENSITIVE_NAMES: &[&str] =
        &["password", "secret", "api_key", "token", "passwd", "apikey"];

    // Assignment'larda: lhs = "string_literal" pattern'ı
    let assign_query = QUERY_ASSIGN_STRINGS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(assignment left: (identifier) @lhs right: (string) @rhs)",
        )
        .expect("Assignment strings sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(assign_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        // Capture 0 = lhs (identifier), Capture 1 = rhs (string)
        if m.captures.len() >= 2 {
            let lhs_node = m.captures[0].node;
            let rhs_node = m.captures[1].node;

            if let (Ok(var_name), Ok(raw_string)) = (
                lhs_node.utf8_text(source_code.as_bytes()),
                rhs_node.utf8_text(source_code.as_bytes()),
            ) {
                // Tırnak işaretlerini soy: "abc" -> abc, 'abc' -> abc
                let content = raw_string
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .trim_start_matches('\'')
                    .trim_end_matches('\'');

                // Boş string'leri atla
                if content.is_empty() {
                    continue;
                }

                if content.starts_with("vault:") || content.starts_with("vault://") {
                    continue;
                }

                let row = lhs_node.start_position().row + 1;

                // Kontrol 1: API key prefix'i var mı?
                let has_prefix = KEY_PREFIXES
                    .iter()
                    .any(|prefix| content.starts_with(prefix));

                // Kontrol 2: Değişken adı hassas mı?
                let var_lower_bytes = var_name.as_bytes();
                let is_sensitive = SENSITIVE_NAMES.iter().any(|sensitive| {
                    // Basit case-insensitive substring kontrolü (clone-free)
                    let s_bytes = sensitive.as_bytes();
                    var_lower_bytes.windows(s_bytes.len()).any(|window| {
                        window
                            .iter()
                            .zip(s_bytes.iter())
                            .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
                    })
                });

                if has_prefix || is_sensitive {
                    secrets.push((var_name, row, content));
                }
            }
        }
    }

    secrets
}

/// YARDIMCI: Bir AST düğümünün alt ağacında belirli türde düğüm olup olmadığını kontrol eder.
/// Recursive DFS ile tarar — sıfır allocation (stack-based).
fn subtree_contains_kind(node: Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if subtree_contains_kind(child, kind) {
                return true;
            }
        }
    }
    false
}

/// KURAL 7: Sonsuz Döngü ve Performans Riski Tespiti
/// `while True:` veya `while 1:` döngülerini bulur ve gövdelerinde
/// `break` veya `return` yoksa potansiyel sonsuz döngü olarak raporlar.
/// Dönen tuple: (koşul &str, satır usize)
pub fn find_infinite_loops<'a, 'b>(
    root_node: Node<'a>,
    source_code: &'b str,
) -> Vec<(&'b str, usize)> {
    let mut loops = Vec::new();

    // while_statement düğümlerini bul
    let while_query = QUERY_WHILE_LOOPS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(while_statement condition: (_) @cond body: (block) @body)",
        )
        .expect("While loops sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(while_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        if m.captures.len() >= 2 {
            let cond_node = m.captures[0].node;
            let body_node = m.captures[1].node;

            if let Ok(cond_text) = cond_node.utf8_text(source_code.as_bytes()) {
                // Sadece "True" veya "1" koşullarını kontrol et
                if cond_text == "True" || cond_text == "1" {
                    let row = cond_node.start_position().row + 1;

                    // Body içinde break veya return var mı?
                    let has_break = subtree_contains_kind(body_node, "break_statement");
                    let has_return = subtree_contains_kind(body_node, "return_statement");

                    if !has_break && !has_return {
                        loops.push((cond_text, row));
                    }
                }
            }
        }
    }

    loops
}

/// KURAL 8: Tehlikeli Dinamik Kod Yürütme Tespiti (Dangerous Executions Detection)
/// `eval()`, `exec()` ve `os.system()` gibi tehlikeli fonksiyon çağrılarını tespit eder.
/// Bu fonksiyonlar rastgele ve potansiyel olarak zararlı kod çalıştırmak için kullanılabilir,
/// özellikle AI tarafından üretilen kod söz konusu olduğunda.
/// Dönen tuple: (düğüm Node, fonksiyon adı &str)
pub fn find_dangerous_executions<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
) -> Vec<(Node<'a>, String)> {
    let mut dangerous_calls = Vec::new();

    // Tehlikeli fonksiyon çağrılarını bul: eval, exec, system,
    // os.system ve subprocess.* process başlatma API'leri.
    let dangerous_query = QUERY_DANGEROUS_EXECUTIONS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            r#"
            [
              (call
                function: (identifier) @func_name)
              (call
                function: (attribute
                  object: (identifier) @obj
                  attribute: (identifier) @method))
            ]
            "#,
        )
        .expect("Tehlikeli fonksiyon çağrıları sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(dangerous_query, root_node, source_code.as_bytes());
    let capture_names = dangerous_query.capture_names();
    let mut shell_true_lines = HashSet::new();

    while let Some(m) = matches.next() {
        let mut identifier_node = None;
        let mut attribute_node = None;
        let mut obj_name = None;
        let mut method_name = None;

        for capture in m.captures {
            let capture_name = capture_names
                .get(capture.index as usize)
                .copied()
                .unwrap_or("");

            match capture_name {
                "func_name" => identifier_node = Some(capture.node),
                "obj" => {
                    obj_name = capture.node.utf8_text(source_code.as_bytes()).ok();
                    attribute_node = capture.node.parent();
                }
                "method" => {
                    method_name = capture.node.utf8_text(source_code.as_bytes()).ok();
                    attribute_node = capture.node.parent();
                }
                _ => {}
            }
        }

        if let Some(node) = identifier_node {
            if let Ok(func_name) = node.utf8_text(source_code.as_bytes()) {
                if matches!(func_name, "eval" | "exec" | "system") {
                    dangerous_calls.push((node, func_name.to_string()));
                }
            }
            continue;
        }

        let Some(obj) = obj_name else {
            continue;
        };
        let Some(method) = method_name else {
            continue;
        };

        let method_is_dangerous = match obj {
            "os" => method == "system",
            "subprocess" => matches!(method, "run" | "Popen" | "call" | "check_output"),
            _ => false,
        };

        if !method_is_dangerous {
            continue;
        }

        let node = attribute_node.unwrap_or_else(|| m.captures[0].node);
        let func_name = format!("{}.{}", obj, method);
        dangerous_calls.push((node, func_name.clone()));

        if let Some(call_node) = node.parent() {
            if call_node.kind() == "call" {
                if let Ok(call_text) = call_node.utf8_text(source_code.as_bytes()) {
                    let compact: String =
                        call_text.chars().filter(|ch| !ch.is_whitespace()).collect();
                    if compact.contains("shell=True")
                        && shell_true_lines.insert(call_node.start_position().row)
                    {
                        dangerous_calls.push((node, format!("{} with shell=True", func_name)));
                    }
                }
            }
        }
    }

    dangerous_calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_comprehension_exclusion() {
        // List comprehension yapısındaki 'x' değişkeninin sahte alarm vermediğini test ediyoruz
        let source = "import json\ndef veri_isle(veri):\n    return [x * 2 for x in veri]";
        let tree = crate::parser::parse_python_code(source).unwrap();
        let root = tree.root_node();

        let hallucinated = vec![];
        let missing_imports = vec![];

        let undefined_vars =
            find_undefined_variables(root, source, &hallucinated, &missing_imports);
        // x sahte hata olarak listeye eklenmemeli, yani tanımlanmamış değişken 0 çıkmalı
        assert_eq!(undefined_vars.len(), 0);
    }

    #[test]
    fn test_hardcoded_secrets() {
        // Güvenlik kuralımızın sızan API keyleri yakaladığını test ediyoruz
        let source = r#"
openai_key = "sk-proj-AiZaFakeKey12345"
API_VAULT_SECRET_REF_AUTH_SHARED_SECRET = "vault:runtime/api_auth_shared_secret"
"#;
        let tree = crate::parser::parse_python_code(source).unwrap();
        let root = tree.root_node();

        let secrets = find_hardcoded_secrets(root, source);
        // 1 tane güvenlik ihlali yakalamış olmalıyız
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].0, "openai_key");
    }

    #[test]
    fn test_infinite_loops() {
        // Çıkışı olmayan while True döngüsünü yakaladığını test ediyoruz
        let source = "while True:\n    print(\"Sonsuz döngü\")";
        let tree = crate::parser::parse_python_code(source).unwrap();
        let root = tree.root_node();

        let loops = find_infinite_loops(root, source);
        // 1 tane sonsuz döngü riski yakalamış olmalıyız
        assert_eq!(loops.len(), 1);
    }
}
