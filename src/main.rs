use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use ai_code_analyzer::{AnalysisOptions, analyze_folder_with_options, generate_html_report};

fn main() {
    println!("🚀 AI Kod Analiz Motoru Başlatılıyor...\n");

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("❌ Hata: Lütfen analiz edilecek klasör yolunu belirtin!");
        eprintln!("Örn: cargo run -- ./my_folder");
        eprintln!("     cargo run -- C:\\path\\to\\python\\files");
        process::exit(1);
    }

    let path_arg = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .unwrap_or_else(|| {
            eprintln!("❌ Hata: Lütfen analiz edilecek klasör yolunu belirtin!");
            process::exit(1);
        });
    let native_only = args.iter().any(|arg| arg == "--native-only");

    if !Path::new(path_arg).is_dir() {
        eprintln!("❌ Hata: '{}' geçerli bir klasör değil!", path_arg);
        process::exit(1);
    }

    let mut options = AnalysisOptions::default();
    options.enable_external_tools = !native_only;

    let report = match analyze_folder_with_options(path_arg, &options) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    if report.total_files_scanned == 0 {
        println!("⚠️  Klasörde Python dosyası bulunamadı: {}", path_arg);
        process::exit(0);
    }

    println!(
        "📂 Klasörde {} adet .py dosyası bulundu.\n",
        report.total_files_scanned
    );
    println!(
        "⚡ PARALEL MOD: Analiz tamamlandı ({} dosya).",
        report.total_files_scanned
    );

    for result in &report.files {
        println!("\n{}", "═".repeat(70));
        println!("📄 Analiz edilen dosya: {}", result.file_path);
        println!("{}", "═".repeat(70));
        println!("{}", result.output_buffer);
    }

    println!("\n\n{}", "═".repeat(70));
    println!("📊 GENEL ANALIZ ÖZETİ (PARALEL MOD)");
    println!("{}", "═".repeat(70));
    println!("✓ Taranılan dosya sayısı  : {}", report.total_files_scanned);
    println!("⚠️  Toplam bulunan hata : {}", report.total_errors_found);
    println!("─────────────────────────────────────────────────────────────────");

    if report.total_errors_found == 0 {
        println!("✅ Mükemmel! Tüm dosyalar temiz!");
    } else {
        println!(
            "⚡ {} adet sorun tespit edildi. Lütfen kontrol edin.",
            report.total_errors_found
        );
    }
    println!("{}", "═".repeat(70));

    let report_dir = PathBuf::from("reports");
    if let Err(e) = fs::create_dir_all(&report_dir) {
        eprintln!("\n❌ Rapor klasörü oluşturulamadı: {}", e);
    }

    generate_html_report(&report);

    match serde_json::to_string_pretty(&report) {
        Ok(json_content) => {
            let json_path = report_dir.join("rapor.json");
            if let Err(e) = fs::write(&json_path, json_content) {
                eprintln!("\n❌ JSON dosyası yazılamadı: {}", e);
            } else {
                println!(
                    "\n✅ JSON rapor başarıyla oluşturuldu: {}",
                    json_path.display()
                );
            }
        }
        Err(e) => eprintln!("\n❌ JSON serileştirme hatası: {}", e),
    }

    if report.total_errors_found > 0 {
        eprintln!("\n⚠️  Program sorunlar buldu. Exit code: 1");
        process::exit(1);
    } else {
        println!("\n✅ Program başarıyla tamamlandı. Exit code: 0");
        process::exit(0);
    }
}
