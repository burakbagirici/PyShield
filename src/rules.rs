use tree_sitter::{Node, Query, QueryCursor};
use std::sync::OnceLock;
// HATA DÜZELTMESİ: tree-sitter üzerinden değil, doğrudan kendi kütüphanesinden çağırıyoruz
use streaming_iterator::StreamingIterator;

// ===== STATIK QUERY OPTIMIZASYONU (Zero-Allocation Queries) =====
// Her query bir kere derlenir ve program ömrü boyunca saklanır

static QUERY_FUNC_CALLS: OnceLock<Query> = OnceLock::new();
static QUERY_FUNC_DEFS: OnceLock<Query> = OnceLock::new();
static QUERY_IMPORTS: OnceLock<Query> = OnceLock::new();
static QUERY_FROM_IMPORTS: OnceLock<Query> = OnceLock::new();
static QUERY_IMPORT_NAMES: OnceLock<Query> = OnceLock::new();
static QUERY_PARAMS: OnceLock<Query> = OnceLock::new();
static QUERY_ASSIGNMENTS: OnceLock<Query> = OnceLock::new();
static QUERY_FOR_LOOPS: OnceLock<Query> = OnceLock::new();
static QUERY_IDENTIFIERS: OnceLock<Query> = OnceLock::new();
static QUERY_ATTRIBUTES: OnceLock<Query> = OnceLock::new();
static QUERY_ASSIGN_STRINGS: OnceLock<Query> = OnceLock::new();
static QUERY_WHILE_LOOPS: OnceLock<Query> = OnceLock::new();
static QUERY_DANGEROUS_EXECUTIONS: OnceLock<Query> = OnceLock::new();

pub const BUILTIN_FUNCTIONS: &[&str] = &[
    "print", "len", "range", "input", "type", "str", "int", "float",
    "list", "dict", "set", "tuple", "bool", "isinstance", "hasattr",
    "getattr", "setattr", "open", "super", "enumerate", "zip", "map",
    "filter", "sorted", "reversed", "abs", "max", "min", "sum", "any",
    "all", "id", "hash", "repr", "hex", "oct", "bin", "chr", "ord",
    "round", "format", "iter", "next", "vars", "dir", "help", "exit",
    "quit", "exec", "eval", "compile", "globals", "locals",
    "staticmethod", "classmethod", "property", "object", "Exception",
    "ValueError", "TypeError", "KeyError", "IndexError", "AttributeError",
    "ImportError", "RuntimeError", "StopIteration", "NotImplementedError",
];

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

/// AKILLI KURAL 3:
/// Kodda import edilen tüm kütüphaneleri bulur.
pub fn find_all_imports<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let mut imports = Vec::new();

    // Basit import'ları tara
    let import_query = QUERY_IMPORTS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(import_statement name: (dotted_name (identifier) @import_name))",
        )
        .expect("Import sorgusu derlenirken hata oluştu!")
    });

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(import_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(import_name) = capture.node.utf8_text(source_code.as_bytes()) {
                imports.push(import_name);
            }
        }
    }

    // from ... import ... yapısını tara
    let from_query = QUERY_FROM_IMPORTS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(import_from_statement module_name: (dotted_name (identifier) @module_name))",
        )
        .expect("From-import sorgusu derlenirken hata oluştu!")
    });

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(from_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(module_name) = capture.node.utf8_text(source_code.as_bytes()) {
                imports.push(module_name);
            }
        }
    }

    imports
}

/// AKILLI KURAL 3 (DEVAM):
/// Kodda tanımlanan tüm değişkenleri bulur (parametreler, assignmentlar, loop vars, imports).
fn find_defined_variables<'a>(root_node: Node<'a>, source_code: &'a str) -> Vec<&'a str> {
    let mut defined_vars = Vec::new();

    // Import statement'larında tanımlanan modüller: import os, import sys
    let import_query = QUERY_IMPORT_NAMES.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(import_statement name: (dotted_name (identifier) @import_name))",
        )
        .expect("Import names sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(import_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(import_name) = capture.node.utf8_text(source_code.as_bytes()) {
                defined_vars.push(import_name);
            }
        }
    }

    // from ... import ... yapısı: from os import path => path tanımlanır
    // Tüm identifier'ları yakala ve parent kontrol et
    let id_query = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(identifier) @id",
        )
        .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(id_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source_code.as_bytes()) {
                // Parent check: import_from_statement altında mı?
                let mut parent_opt = capture.node.parent();
                let mut found = false;
                
                // Maximum 3 level up çık (parent -> grandparent -> great-grandparent)
                for _ in 0..3 {
                    if let Some(parent) = parent_opt {
                        if parent.kind() == "import_from_statement" {
                            // from ... import ... inside, ekle
                            found = true;
                            break;
                        }
                        // Kısa circuit: import statement ise this is regular import, çık
                        if parent.kind() == "import_statement" || parent.kind() == "dotted_name" {
                            break;
                        }
                        parent_opt = parent.parent();
                    } else {
                        break;
                    }
                }
                
                if found {
                    defined_vars.push(name);
                }
            }
        }
    }


    // Fonksiyon parametreleri: def func(x, y, z)
    let param_query = QUERY_PARAMS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(parameters (identifier) @param)",
        )
        .expect("Params sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(param_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(param_name) = capture.node.utf8_text(source_code.as_bytes()) {
                defined_vars.push(param_name);
            }
        }
    }

    // Assignment'lar: x = 10
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
            if let Ok(var_name) = capture.node.utf8_text(source_code.as_bytes()) {
                defined_vars.push(var_name);
            }
        }
    }

    // For loop değişkenleri: for i in range(10)
    let for_query = QUERY_FOR_LOOPS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(for_statement left: (identifier) @loop_var)",
        )
        .expect("For loops sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(for_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(loop_var) = capture.node.utf8_text(source_code.as_bytes()) {
                defined_vars.push(loop_var);
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

/// AKILLI KURAL 3 (İŞLEV):
/// Kodda kullanılan tanımlanmamış değişkenleri bulur.
/// Daha akıllı filtering ile false positive'leri azaltır.
/// MÜKEMMELLEŞTİRME: hallucinated ve missing_imports listelerindeki isimler
/// mükerrer raporlamayı önlemek için burada filtrelenir.
pub fn find_undefined_variables<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
    hallucinated: &[&'a str],
    missing_imports: &[&'a str],
) -> Vec<(Node<'a>, &'a str)> {
    let defined = find_defined_variables(root_node, source_code);

    // Function definition names'ı topla (bunlar değişken değil)
    let func_names = find_function_definitions(root_node, source_code);

    // Tüm identifier usageları: herhangi bir identifier context'te
    let usage_query = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(identifier) @id",
        )
        .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });
    
    let mut undefined_vars = Vec::new();

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(usage_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(var_name) = capture.node.utf8_text(source_code.as_bytes()) {
                // Comprehension içindeyse atla
                if is_inside_comprehension(capture.node) {
                    continue;
                }

                // Node'un parent'ını kontrol et (import, function def, attribute vb)
                let parent = capture.node.parent();
                
                // İlgisiz parent types'ını skip et
                if let Some(p) = parent {
                    let parent_kind = p.kind();
                    
                    // Import statement içindeki identifiers skip
                    if parent_kind == "import_statement" || parent_kind == "import_from_statement" 
                        || parent_kind == "dotted_name" {
                        continue;
                    }
                    
                    // Function definition adı skip
                    if parent_kind == "function_definition" {
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

                // Eğer tanımlanmamışsa ve built-in değilse, undefined
                if !defined.contains(&var_name) 
                    && !BUILTIN_FUNCTIONS.contains(&var_name)
                    && !func_names.contains(&var_name) {
                    
                    // Keywords'ü skip et
                    if var_name != "def" && var_name != "for" && var_name != "in" 
                        && var_name != "return" && var_name != "import" {

                        // MÜKEMMELLEŞTİRME: Zaten halüsinasyon veya eksik import
                        // olarak raporlandıysa, bu listeye ekleme (mükerrer önleme)
                        if !hallucinated.contains(&var_name) 
                            && !missing_imports.contains(&var_name) {
                            undefined_vars.push((capture.node, var_name));
                        }
                    }
                }
            }
        }
    }

    undefined_vars
}

/// KURAL 5: Kullanılmayan Kod Tespiti (Unused Code Detector)
/// Tanımlanmış ama hiç kullanılmayan import'ları ve değişkenleri bulur.
/// Dönen tuple: (ad &str, satır numarası usize, kategori &str)
pub fn find_unused_code<'a, 'b>(
    root_node: Node<'a>,
    source_code: &'b str,
) -> Vec<(&'b str, usize, &'static str)> {
    let mut unused = Vec::new();

    // --- Kullanılan tüm identifier'ları topla ---
    // (Sadece "kullanım" context'lerinden: atama solu değil, import tanımı değil)
    let mut used_names: Vec<&'b str> = Vec::new();

    let id_query = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(identifier) @id",
        )
        .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(id_query, root_node, source_code.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source_code.as_bytes()) {
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
    // import os  -> os hiç kullanılmıyor mu?
    let import_query = QUERY_IMPORTS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(import_statement name: (dotted_name (identifier) @import_name))",
        )
        .expect("Import sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(import_query, root_node, source_code.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(imp_name) = capture.node.utf8_text(source_code.as_bytes()) {
                let row = capture.node.start_position().row + 1;
                if !used_names.contains(&imp_name) {
                    unused.push((imp_name, row, "import"));
                }
            }
        }
    }

    // from os import path -> path hiç kullanılmıyor mu?
    // Tüm identifier'ları yakala ve parent kontrol et
    let id_query2 = QUERY_IDENTIFIERS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(identifier) @id",
        )
        .expect("Identifiers sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(id_query2, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source_code.as_bytes()) {
                // Parent check: import_from_statement altında mı?
                let mut parent_opt = capture.node.parent();
                let mut found = false;
                
                // Maximum 3 level up çık
                for _ in 0..3 {
                    if let Some(parent) = parent_opt {
                        if parent.kind() == "import_from_statement" {
                            found = true;
                            break;
                        }
                        if parent.kind() == "import_statement" || parent.kind() == "dotted_name" {
                            break;
                        }
                        parent_opt = parent.parent();
                    } else {
                        break;
                    }
                }
                
                if found {
                    let row = capture.node.start_position().row + 1;
                    if !used_names.contains(&name) {
                        unused.push((name, row, "import"));
                    }
                }
            }
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
            if let Ok(var_name) = capture.node.utf8_text(source_code.as_bytes()) {
                let row = capture.node.start_position().row + 1;
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

    unused
}

/// AKILLI KURAL 4:
/// Kodda noktalı notasyon ile kullanılan ama import edilmemiş kütüphaneleri bulur.
/// Örn: math.sqrt() veya os.path - math ve os import edilmemişse hata.
pub fn find_missing_imports<'a>(
    root_node: Node<'a>,
    source_code: &'a str,
    imports: &[&'a str],
) -> Vec<(Node<'a>, &'a str)> {
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
                // Eğer library'nin adı import edilmemişse, bu eksik import
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
    const SENSITIVE_NAMES: &[&str] = &["password", "secret", "api_key", "token", "passwd", "apikey"];

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

                let row = lhs_node.start_position().row + 1;

                // Kontrol 1: API key prefix'i var mı?
                let has_prefix = KEY_PREFIXES.iter().any(|prefix| content.starts_with(prefix));

                // Kontrol 2: Değişken adı hassas mı?
                let var_lower_bytes = var_name.as_bytes();
                let is_sensitive = SENSITIVE_NAMES.iter().any(|sensitive| {
                    // Basit case-insensitive substring kontrolü (clone-free)
                    let s_bytes = sensitive.as_bytes();
                    var_lower_bytes.windows(s_bytes.len()).any(|window| {
                        window.iter().zip(s_bytes.iter()).all(|(a, b)| {
                            a.to_ascii_lowercase() == b.to_ascii_lowercase()
                        })
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
) -> Vec<(Node<'a>, &'a str)> {
    let mut dangerous_calls = Vec::new();

    // Tehlikeli fonksiyon çağrılarını bul: eval, exec, system
    let dangerous_query = QUERY_DANGEROUS_EXECUTIONS.get_or_init(|| {
        Query::new(
            &tree_sitter_python::LANGUAGE.into(),
            "(call function: (identifier) @func_name (#match? @func_name \"^(eval|exec|system)$\"))",
        )
        .expect("Tehlikeli fonksiyon çağrıları sorgusu derlenirken hata oluştu!")
    });

    let mut qc = QueryCursor::new();
    let mut matches = qc.matches(dangerous_query, root_node, source_code.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(func_name) = capture.node.utf8_text(source_code.as_bytes()) {
                dangerous_calls.push((capture.node, func_name));
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
        
        let undefined_vars = find_undefined_variables(root, source, &hallucinated, &missing_imports);
        // x sahte hata olarak listeye eklenmemeli, yani tanımlanmamış değişken 0 çıkmalı
        assert_eq!(undefined_vars.len(), 0);
    }

    #[test]
    fn test_hardcoded_secrets() {
        // Güvenlik kuralımızın sızan API keyleri yakaladığını test ediyoruz
        let source = "openai_key = \"sk-proj-AiZaFakeKey12345\"";
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