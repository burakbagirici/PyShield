use tree_sitter::{Parser, Tree};

/// Dışarıdan gelen kaynak kod string'ini (&str olarak borçlanarak) alır
/// ve Tree-sitter kullanarak bir AST (Ağaç) yapısına dönüştürür.
pub fn parse_python_code(source_code: &str) -> Option<Tree> {
    // 1. Yeni bir parser nesnesi oluşturuyoruz
    let mut parser = Parser::new();

    // 2. Parser'a Python dilinin kurallarını yükliyoruz
    // (Eğer dil yüklenemezse None döner)
    let language = tree_sitter_python::LANGUAGE;
    if parser.set_language(&language.into()).is_err() {
        return None;
    }

    // 3. Kaynak kodu ayrıştırıp ağaç yapısını (Tree) üretiyoruz
    // Burada kaynak kod kopyalanmaz, sadece referans alınır (&str)
    let parsed_tree = parser.parse(source_code, None);

    parsed_tree
}