# PyShield - Rust Tabanlı Python AST Analiz Motoru 🛡️🚀

Bu proje; Python kaynak kodlarındaki siber güvenlik ihlallerini, yapay zeka halüsinasyonlarını (uydurma fonksiyon çağrıları), eksik importları, tanımsız değişkenleri ve sonsuz döngü risklerini tespit eden **Tauri v2** ve **Rust** tabanlı modern bir masaüstü uygulamasıdır.

## 🛠️ Kullanılan Teknolojiler

- **Backend (Analiz Motoru):** Rust, `tree-sitter-python` (Abstract Syntax Tree - AST Parsing), `rayon` (Çoklu çekirdekle paralel dosya tarama)
- **Frontend (Kullanıcı Arayüzü):** HTML5, Tailwind CSS, Vanilla JavaScript
- **Framework:** Tauri v2 (Sıfır sunucu maliyeti, maksimum veri gizliliği sağlayan _Local-First_ mimari)

## 🎯 Öne Çıkan Özellikler & Stres Testleri

- **Yüksek Performans:** Rust ve `rayon` kütüphanesinin paralel işlem gücü sayesinde saniyede binlerce satır kod analizi.
- **Gerçek Dünya Projesi (Flask) Testi:** Devasa açık kaynak kodlu **Flask Framework** kaynak kodları (83 dosya, 6716 bulgu) üzerinde test edilmiş; motor kilitlenmeden veya çökmeden tüm zafiyetleri ve kod kokularını (code smell) başarıyla listelemiştir.
- **Robustness (Kurşun Geçirmezlik):** Boş klasör senaryoları ve Windows üzerindeki Türkçe karakterli/boşluklu dosya yolları (`Yapay Zeka Şampiyonu Örnek Klasörü`) altındaki taramalar hata yakalama mekanizmalarıyla (error handling) kusursuz şekilde yönetilmektedir.
- **Hassas Konumlandırma:** Tespit edilen her bulgu; dosya adı, satır numarası, sütun numarası, ilgili kod satırının önizlemesi ve son kullanıcı dostu açıklama kartlarıyla görselleştirilmiştir.

## 🚀 Projeyi Yerelde Çalıştırma

Projenin çalışabilmesi için bilgisayarınızda **Rust** ve **Node.js** yüklü olmalıdır.

1. Bağımlılıkları yükleyin:
   ```bash
   cargo tauri build --dry-run
   ```
