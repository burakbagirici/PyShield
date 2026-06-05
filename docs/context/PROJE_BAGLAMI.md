# Proje: Çok Dilli AI Kod Analiz Motoru (Rust)

## Mimari Kurallar (Baş Mühendis Talimatları)

1. **Dilden Bağımsız Altyapı:** Kod analizinde dile özgü lokal parserlar yerine `tree-sitter` ve ortak Query (S-Expression) mimarisi kullanılacaktır.
2. **Hafıza Yönetimi:** Gereksiz `.clone()` kullanımından kaçınılacak, kaynak kod string'leri fonksiyonlara borçlanma (`&str`) yöntemiyle paslanacaktır.
3. **Modülerlik:** Tüm mantık tek bir dosyaya yığılmayacak; `parser`, `rules` ve `reporter` olarak modüllere ayrılacaktır.

## Mevcut Aşama: Faz 0 - Teorik Hazırlık ve Kurulum

- [x] Rust Ortamı Kurulumu (rustc 1.95.0)
- [x] `cargo new ai_code_analyzer` ile projenin oluşturulması
- [ ] Geliştirici Udemy Kursu Teorik Eğitim Süreci (Devam Ediyor)

## Agent'a Talimat

Bu dosyadaki kurallara kesinlikle sadık kal. Kullanıcı senden kod yazmanı istediğinde sadece ilgili modülü güncelle, tüm projeyi baştan üreterek token israfı yapma.
