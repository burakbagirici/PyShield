# Task2 - EPIC ve BL Planı

Tarih: 2026-06-04  
Kaynaklar: `Task0-Değerlendirme.md`, `Task1-Eksikler ve Knumlandırma.md`, mevcut repo analizi  
Doğrulama: `cargo test` 12/12 başarılı; `cargo check --manifest-path src-tauri\Cargo.toml` başarılı; frontend script syntax check başarılı; `npm run tauri build` app.exe, MSI ve NSIS bundle üretti.

## Kısa Konumlandırma

PyShield, Ruff/Pyright/Bandit/Semgrep gibi olgun araçların yerine geçmeye çalışmamalı. Ürünün doğru yönü:

> AI ile üretilen Python kodunu lokal olarak tarayan, hızlı Tree-sitter ön analizi yapan, harici analiz araçlarının bulgularını tek şemada birleştiren ve bunları Tauri masaüstü arayüzünde açıklanabilir risk raporuna çeviren local-first analiz platformu.

Mevcut repo bu yönde iyi bir prototip seviyesinde:

- Rust analiz motoru var: `src/lib.rs`, `src/parser.rs`, `src/rules.rs`
- Tree-sitter tabanlı Python AST analizi var.
- Paralel dosya tarama var.
- JSON/HTML rapor üretimi var.
- Tauri v2 köprüsü ve frontend var.
- `Cargo.toml` mevcut; Task0 içindeki "Cargo.toml yok" notu artık güncel değil.

Ana açıklar ise doğruluk, normalleştirilmiş bulgu şeması, güvenli raporlama, exclude stratejisi ve external tool adapter katmanı tarafında.

## EPIC-001 - PyShield v0.2 Güvenilir Local-first AI Python Risk Scanner

Amaç: PyShield'ı demo seviyesindeki AST tarayıcıdan, güvenilir çıktı üreten ve büyümeye hazır local-first masaüstü analiz ürününe taşımak.

Başarı kriterleri:

- Her bulgu zorunlu `severity`, `rule_id`, `tool`, `category`, `message`, `file`, `line`, `column`, `code_snippet` alanlarıyla raporlanır.
- Default taramada `.git`, `.venv`, `venv`, `__pycache__`, `site-packages`, `node_modules`, `target` gibi klasörler dışlanır.
- Import alias, attribute call ve temel Python scope senaryoları false-positive/false-negative üretmeyecek seviyeye getirilir.
- `os.system`, `subprocess.run`, `subprocess.Popen`, `eval`, `exec` gibi riskli çağrılar kaçırılmaz.
- HTML/Frontend tarafında tüm kullanıcı kontrollü alanlar escape edilir.
- Ruff, Bandit ve Pyright için adapter mimarisi hazırlanır.
- CLI, JSON, HTML ve Tauri çıktıları aynı normalize edilmiş bulgu modelini kullanır.

## P0 - Güven ve Çekirdek Doğruluk

P0 maddeleri ürünün yanlış güven hissi vermemesi için önce yapılmalıdır.

Kalibrasyon Notu: Sprint 1/2 tamamlandıktan sonra `D:\PROGRAMMING\abey` analizi ilk koşuda 15181 bulgu üretmişti. Generated/Pydantic class field'ların unused sayılması, typed parameter'ların local scope'a alınmaması ve class adlarının annotation bağlamında tanımsız görünmesi düzeltildi. Yeniden analizde toplam bulgu 1764'e düştü; `generated_db_models.py`, `accounting.py` ve `document_platform_repository.py` örnekleri 0 false-positive seviyesine indi. Doğrulama: `cargo test` 11 test başarılı.

### BL-P0-001 - Normalize Finding Schema

Status: Tamamlandı

Tamamlanma Notu: `Issue` modeli `file_path`, `tool`, `rule_id`, zorunlu `severity`, `confidence` ve opsiyonel `fix_hint` alanlarıyla genişletildi. PyShield bulguları artık sabit rule id'lerle ve normalize şemayla JSON/HTML/Tauri akışına giriyor.

Problem: `Issue` içinde `severity` opsiyonel; `rule_id`, `tool`, `confidence`, `fix_hint` yok. Bu durum CLI, JSON, HTML ve Tauri arayüzünde tutarlı sınıflandırmayı zorlaştırıyor.

Yapılacaklar:

- `Issue` modelini zorunlu alanlarla genişlet.
- `severity` değerlerini enum mantığıyla sınırla: `ERROR`, `WARNING`, `INFO`.
- PyShield kuralları için sabit `rule_id` standardı oluştur:
  - `PYSHIELD_SYNTAX_001`
  - `PYSHIELD_AI_FUNC_001`
  - `PYSHIELD_IMPORT_001`
  - `PYSHIELD_SCOPE_001`
  - `PYSHIELD_SECRET_001`
  - `PYSHIELD_DANGEROUS_EXEC_001`
  - `PYSHIELD_LOOP_001`
- `tool = "pyshield"` alanını ekle.

Kabul kriterleri:

- JSON çıktısındaki her issue aynı şemaya uyar.
- Frontend eski ve yeni alanları kırılmadan gösterebilir.
- `cargo test` başarılı geçer.

### BL-P0-002 - Merkezi HTML/Output Escaping

Status: Tamamlandı

Tamamlanma Notu: Rust tarafına merkezi `escape_html()` helper'ı eklendi; `generate_html_report()` içindeki proje yolu, timestamp, status, dosya yolu ve output buffer gibi dinamik alanlar escape ediliyor. Frontend issue kartları da yeni alanları güvenli `escapeHtml()` ile gösteriyor.

Problem: `output_buffer` için kısmi escaping var; `project_path`, `file_path`, timestamp, issue mesajları ve snippet alanları merkezi şekilde escape edilmiyor.

Yapılacaklar:

- Rust tarafında `escape_html()` yardımcı fonksiyonu ekle.
- `generate_html_report()` içindeki tüm dinamik alanları bu fonksiyondan geçir.
- Frontend tarafında mevcut `escapeHtml()` kullanımını koru ve issue alanlarının tamamında kullanıldığını test et.

Kabul kriterleri:

- Path veya kod snippet içine `<script>` benzeri içerik girildiğinde HTML çalıştırılmaz, metin olarak görünür.
- HTML rapor ve Tauri UI aynı güvenlik davranışını verir.

### BL-P0-003 - Default Exclude Stratejisi

Status: Tamamlandı

Tamamlanma Notu: `AnalysisOptions` ve `DEFAULT_EXCLUDED_DIRS` eklendi. Varsayılan recursive tarama `.git`, `.venv`, cache, `.tmp`, `.tox`, `tmp`, `temp`, `pytesttemp`, `site-packages`, `node_modules`, `target`, `dist`, `build` ve `agent-logs` klasörlerini atlıyor. Exclude davranışı integration test ile doğrulandı.

Problem: Recursive tarama her klasöre giriyor. Büyük repo, sanal ortam, cache ve build klasörleri gereksiz bulgu ve performans sorunu üretir.

Yapılacaklar:

- Default dışlama listesi ekle:
  - `.git`
  - `.venv`
  - `venv`
  - `env`
  - `__pycache__`
  - `.mypy_cache`
  - `.ruff_cache`
  - `site-packages`
  - `node_modules`
  - `target`
  - `dist`
  - `build`
  - `agent-logs`
- CLI için ileride genişletilebilir `AnalysisOptions` modeli tasarla.
- Tauri komutunun ileride options alabilecek şekilde evrilmesini kolaylaştır.

Kabul kriterleri:

- Excluded klasörlerdeki `.py` dosyaları taranmaz.
- Windows path ve Türkçe karakterli klasörlerde tarama bozulmaz.

### BL-P0-004 - Attribute Call ve Tehlikeli Çalıştırma Tespiti

Status: Tamamlandı

Tamamlanma Notu: `find_dangerous_executions()` artık `eval`, `exec`, direct `system`, `os.system`, `subprocess.run`, `subprocess.Popen`, `subprocess.call`, `subprocess.check_output` ve `shell=True` kullanımını yakalıyor. Dangerous fixture regression testi eklendi.

Problem: Mevcut `find_dangerous_executions()` identifier call yakalıyor; `os.system()` ve `subprocess.run()` gibi attribute call senaryoları kaçabilir.

Yapılacaklar:

- Attribute call query ekle:
  - `os.system(...)`
  - `subprocess.run(...)`
  - `subprocess.Popen(...)`
  - `subprocess.call(...)`
  - `subprocess.check_output(...)`
- `shell=True` kullanımını ayrıca risk olarak işaretle.
- `eval()` ve `exec()` tespitini koru.

Kabul kriterleri:

- `test_dangerous.py`, `test_dangerous_integration.py`, `test_dangerous_code/dangerous.py` içindeki riskli çağrılar yakalanır.
- Güvenli `print()` ve `len()` çağrıları security bulgusu üretmez.

### BL-P0-005 - Import Alias ve Imported Symbol Tablosu

Status: Tamamlandı

Tamamlanma Notu: `ImportSymbol` modeli ve `collect_import_symbols()` eklendi. `import x as y`, `from x import y`, `from x import y as z` ve wildcard import bilgileri local sembol tablosuna taşınıyor; halüsinasyon, eksik import, undefined ve unused import analizleri bu tabloyu kullanıyor.

Problem: `import numpy as np`, `from pathlib import Path as P`, `from math import sqrt` gibi yaygın Python importları halüsinasyon, eksik import ve tanımsız değişken analizinde yanlış sonuç üretebilir.

Yapılacaklar:

- Importları sadece string listesi olarak değil, sembol tablosu olarak modelle:
  - `module`
  - `imported_name`
  - `local_name`
  - `is_wildcard`
  - `line`
- `defined_variables` içine alias isimlerini ekle.
- Halüsinasyon analizinde `imported_symbols` değerlerini tanımlı kabul et.
- Kullanılmayan import analizinde alias kullanımını doğru say.

Kabul kriterleri:

- `import numpy as np; np.array([])` eksik import veya tanımsız değişken vermez.
- `from math import sqrt; sqrt(16)` halüsinasyon vermez.
- `from pathlib import Path as P; P(".")` doğru kabul edilir.

### BL-P0-006 - Temel Scope İyileştirmeleri

Status: Tamamlandı

Tamamlanma Notu: Assignment ve for-loop binding çıkarımı tuple/list unpacking destekleyecek şekilde ortak helper'a taşındı. `with ... as ...` aliasları local tanım kabul ediliyor; local değişken attribute'ları artık eksik import olarak raporlanmıyor. Clean alias/scope fixture testi eklendi.

Problem: Python scope analizi hâlâ yüzeysel. `with as`, tuple unpacking, for unpacking ve class method senaryolarında hatalı bulgular çıkabilir.

Yapılacaklar:

- `with open(...) as f` içindeki `f` tanımlı kabul edilsin.
- `a, b = get_values()` tuple/list unpacking desteklensin.
- `for key, value in items:` unpacking desteklensin.
- `except Error as e` desteği korunup testle sabitlensin.
- `self.attr` ve `cls.attr` muafiyeti korunur.

Kabul kriterleri:

- False-positive fixture dosyası Rust testlerine taşınır.
- Bu senaryolar tanımsız değişken üretmez.

### BL-P0-007 - Test Fixture ve Regression Paketi

Status: Tamamlandı

Tamamlanma Notu: `tests/fixtures/` altında P0 fixture seti ve `tests/p0_regressions.rs` integration testleri eklendi. Fixture paketi clean alias/scope, dangerous execution, missing import, syntax error, wildcard import ve default exclude senaryolarını kapsıyor.

Problem: Repo içinde Python fixture dosyaları var ama Rust test paketi sadece 3 unit testten oluşuyor.

Yapılacaklar:

- `tests/fixtures/` altında pozitif ve negatif Python örneklerini düzenle.
- `analyze_folder()` veya daha küçük rule fonksiyonları için integration test ekle.
- Minimum fixture grupları:
  - false-positive prevention
  - dangerous execution
  - import alias
  - missing import
  - hardcoded secret
  - syntax error
  - wildcard import

Kabul kriterleri:

- `cargo test` gerçek fixture setini koşturur.
- Yeni P0 kuralları regression testi olmadan merge edilmez.

## P1 - Ürünleşme ve Araç Orkestrasyonu

P1 maddeleri PyShield'ı sadece kendi kurallarına bağlı kalmayan, açık kaynak analiz ekosistemini kullanan ürüne dönüştürür.

Araç Seti Notu: Ruff, Bandit, Pyright ve GitNexus hedef repoya kurulu olmak zorunda değildir. PyShield kendi managed toolchain alanını kullanır: `.pyshield-tools/python` ve `.pyshield-tools/node`. `npm run tools:install` bu toolchain'i yeniden kurar. `PYSHIELD_TOOLS_DIR` ile paketlenmiş/özel araç dizini override edilebilir. Hedef repo `.venv` ve `node_modules` araması yalnızca fallback'tir. GitNexus analiz bulgusu üretmez; hedef repo index durumunu `tool_statuses` içinde gösterir.

Doğrulama Notu: `D:\PROGRAMMING\abey` managed toolchain ile analiz edildi. Native PyShield bulgusu 1764 seviyesinden 892'ye, halüsinasyon bulguları 887'den 15'e düştü. External adapterlar `gitnexus: ok`, `ruff: ok / 0`, `bandit: ok / 129`, `pyright: ok / 691` olarak rapora merge edildi. Test dosyalarındaki Bandit `B101` assert gürültüsü filtrelendi, `pyproject.toml` içinde tanımlı dependency'ler için Pyright missing-import false-positive'leri bastırıldı, constructor/class call ve callable parametre false-positive'leri azaltıldı, Windows `\\?\` path normalizasyonu düzeltildi. Hızlı Rust scan 14.1 sn / 892 bulgu, tam toolchain scan 31.9 sn / 1712 bulgu üretti. `.tmp`, `.tox`, `tmp`, `temp` ve `pytesttemp` default exclude listesine eklendikten sonra Abey pytest temp erişim uyarıları temizlendi.

### BL-P1-001 - AnalyzerAdapter Katmanı

Status: Tamamlandı

Tamamlanma Notu: `src/adapters.rs` altında `AnalyzerAdapter` trait'i eklendi. PyShield native analiz paralel Rust akışı olarak korunurken Ruff/Bandit/Pyright harici adapterları rapora sonradan merge ediliyor. `tool_statuses` alanı eklendi; tool yoksa analiz kırılmadan `unavailable` bilgisi dönüyor. Araç keşfi PyShield managed toolchain öncelikli hale getirildi; hedef repo içindeki tool kurulumları sadece fallback olarak kullanılıyor.

Yapılacaklar:

- Ortak trait tasarla:

```rust
pub trait AnalyzerAdapter {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn run(&self, project_path: &str) -> Result<Vec<Issue>, String>;
}
```

- İlk adapterlar:
  - `PyShieldAdapter`
  - `RuffAdapter`
  - `BanditAdapter`
  - `PyrightAdapter`
- Harici araç yoksa analiz kırılmasın; `tool_unavailable` bilgisi rapora eklensin.

Kabul kriterleri:

- Her adapter normalize `Issue` üretir.
- Harici tool bulunmadığında kullanıcıya net bilgi verilir.

### BL-P1-002 - Ruff Entegrasyonu

Status: Tamamlandı

Tamamlanma Notu: `RuffAdapter`, PyShield managed toolchain/PATH/fallback adaylarını deneyerek `ruff check . --output-format json --exit-zero` çıktısını normalize `Issue` şemasına çeviriyor. Ruff rule id'leri `Kullanılmayan Kod`, `Tanımsız Değişken`, `Eksik Import`, `Sözdizimi` veya `Lint` kategorilerine map ediliyor. Ruff 0.15.15 `.pyshield-tools/python` altına kuruldu.

Amaç: Genel lint, unused import, unused variable ve style tarafında tekerleği yeniden icat etmemek.

Yapılacaklar:

- `ruff check --output-format json` çıktısını parse et.
- Ruff bulgularını `tool = "ruff"` olarak normalize et.
- PyShield custom rule bulgularıyla birlikte tek raporda göster.

Kabul kriterleri:

- Ruff kuruluysa rapora dahil olur.
- Ruff kurulu değilse PyShield native analiz çalışmaya devam eder.

### BL-P1-003 - Bandit Entegrasyonu

Status: Tamamlandı

Tamamlanma Notu: `BanditAdapter`, PyShield managed toolchain üzerinden `bandit -r . -f json -q` çıktısını parse edip Bandit `test_id`, severity, satır ve snippet bilgisini `Güvenlik İhlali` kategorisine normalize ediyor. Native PyShield security bulgularıyla aynı dosya bazlı rapor yapısına merge ediliyor. Bandit 1.9.4 `.pyshield-tools/python` altına kuruldu.

Amaç: Güvenlik bulgularını olgun Python SAST kurallarıyla desteklemek.

Yapılacaklar:

- `bandit -r -f json` çıktısını parse et.
- Severity mapping tanımla.
- PyShield native security quick scan ile mükerrer bulgular için dedup stratejisi ekle.

Kabul kriterleri:

- Bandit sonuçları güvenlik kategorisinde görünür.
- Aynı satırdaki aynı risk iki kez ağırlaştırılmış skor üretmez.

### BL-P1-004 - Pyright Entegrasyonu

Status: Tamamlandı

Tamamlanma Notu: `PyrightAdapter`, PyShield managed node toolchain üzerinden `pyright . --outputjson` çıktısını parse ediyor. `reportMissingImports` eksik import, `reportUndefinedVariable` tanımsız değişken, diğer teşhisler `Type Checking` kategorisine map ediliyor ve aynı normalize issue şemasına dahil ediliyor. Pyright 1.1.410 `.pyshield-tools/node` altına kuruldu.

Amaç: Tanımsız sembol ve type-checking tarafında PyShield'ın yüzeysel scope analizini olgun bir araçla desteklemek.

Yapılacaklar:

- Pyright JSON çıktısını normalize et.
- Type ve undefined symbol bulgularını ayrı kategoride göster.
- PyShield custom hallucination rule ile çakışan bulguları ilişkilendir.

Kabul kriterleri:

- Pyright bulguları ayrı `tool = "pyright"` olarak gelir.
- PyShield bulguları silinmez; ilişkili risk olarak gruplanır.

### BL-P1-005 - Dependency Awareness

Amaç: AI kodlarında sık görülen "kodda paket kullanılmış ama bağımlılık dosyasında yok" hatasını yakalamak.

Yapılacaklar:

- Şu dosyaları okuyacak dependency scanner ekle:
  - `requirements.txt`
  - `pyproject.toml`
  - `Pipfile`
  - `poetry.lock`
  - `setup.py`
  - `environment.yml`
- Import edilen top-level paketlerle dependency listelerini karşılaştır.

Kabul kriterleri:

- `import pandas as pd` var ama dependency dosyasında `pandas` yoksa bulgu üretilir.
- Standart library modülleri dependency eksikliği olarak raporlanmaz.

### BL-P1-006 - Tauri UI Filtre ve Detay Deneyimi

Status: Tamamlandı

Tamamlanma Notu: Tauri frontend'e tool status paneli, severity/tool/category select filtreleri ve dosya path arama filtresi eklendi. Issue kartları `severity`, `tool`, `rule_id`, snippet, mesaj ve `fix_hint` gösteriyor; filtreler aynı raporu yeniden render ederek dosya accordion'unu daraltıyor.

Kapanış Notu: BL-P1-006 kod tarafında `src-frontend/index.html` içinde tamamlandı. Aynı analiz raporunda tool status görünümü, boş sonuç mesajı, severity/tool/category filtreleri ve dosya yolu araması aktif çalışacak şekilde bağlandı.

Build Notu: Root `package.json` eklendi; `npm run tauri build` komutu Tauri CLI 2.11.2 ile çalışıyor. Güncel build çıktıları `src-tauri/target/release/app.exe`, `src-tauri/target/release/bundle/msi/PyShield_0.1.0_x64_en-US.msi` ve `src-tauri/target/release/bundle/nsis/PyShield_0.1.0_x64-setup.exe`.

Son Build Notu: 2026-06-05 final koşusunda `cargo test` 12/12 geçti. `npm run tauri build` uyarısız şekilde release binary, MSI ve NSIS installer üretti.

Frontend Güncelleme Notu: Analiz modu seçimi eklendi: hızlı Rust scan veya tam toolchain scan. GitNexus paneli frontend'e eklendi; seçili klasör için status yenileme, dosya enjeksiyonsuz analyze ve query komutları Tauri üzerinden çalışıyor.

Yeniden Tasarım Kapanış Notu: 2026-06-05 tarihinde frontend prototip yapıdan tam PyShield Workbench uygulama kabuğuna taşındı. Sidebar navigasyonu, Overview, Issues, GitNexus Graph, AI Workbench ve Settings ekranları `src-frontend/index.html` içinde kodlandı. Tauri backend'e SQLite provider ayarları, OpenAI/DeepSeek/OpenAI-compatible provider listesi, API key kaydetme/silme ve AI + GitNexus workflow komutları eklendi. GitNexus query çağrısı path yerine registry repo etiketi kullanacak şekilde düzeltildi; index komutuna `force` seçeneği eklendi.

Yapılacaklar:

- Severity filtreleri: Error, Warning, Info.
- Tool filtreleri: PyShield, Ruff, Bandit, Pyright.
- Dosya bazlı arama ve kategori filtreleri.
- Issue detayında rule id, tool, satır/sütun, snippet ve fix hint gösterimi.

Kabul kriterleri:

- Kullanıcı aynı analiz raporunda araç, kategori ve severity bazında filtreleme yapabilir.
- Boş sonuç ve tool unavailable durumları düzgün görünür.

### BL-P1-007 - Frontend Workbench Ürün Planı

Status: Tamamlandı

Tamamlanma Notu: `src-frontend/index.html` tek sayfalı Tauri arayüzü olarak yeniden tasarlandı. Kullanıcı ilk ekranda doğrudan proje path'i, scan modu ve Run Scan komutunu görür; sidebar ile Overview, Issues, GitNexus Graph, AI Workbench ve Settings ekranlarına geçer. UI artık ham JSON basmak yerine tool status, issue table, inspector, GitNexus node canvas, raw output ve AI summary alanlarını ayrıştırıyor.

Plan:

- App shell: sol sidebar, üst command bar, proje path input'u, klasör seçme, Native/Full Toolchain scan modu.
- Overview: metrikler, PyShield/Ruff/Bandit/Pyright/GitNexus status strip, hızlı issue triage ve selected issue inspector.
- Issues: severity/tool/category filtreleri, path/rule/message araması ve detay inspector.
- GitNexus Graph: `status`, `analyze --skip-git --skip-agents-md --skip-skills`, `analyze --force --skip-git --skip-agents-md --skip-skills`, `query`, limit seçimi, JSON raw output ve node/edge SVG canvas.
- AI Workbench: konu prompt'u, provider seçimi, GitNexus query + LLM özet akışı.
- Settings: OpenAI, DeepSeek ve OpenAI-compatible provider kartları; model, base URL ve API key yönetimi.
- Responsive davranış: desktop iki kolon, dar ekranda sidebar üst navigasyona ve paneller tek kolona düşer.

Kabul kriterleri:

- Kullanıcı GitNexus komutlarını frontend'den çalıştırabilir.
- Query çıktısı JSON ise node/edge görünümüne dönüştürülür; warning varsa inspector alanında görünür.
- API key UI'a geri basılmaz; yalnızca `API Key Saved` durumu gösterilir.
- Frontend external CDN gerektirmez; CSS/JS local tek dosyadadır.

### BL-P1-008 - SQLite Provider Ayarları ve AI + GitNexus Workflow

Status: Tamamlandı

Tamamlanma Notu: Tauri backend'e `list_ai_provider_configs`, `save_ai_provider_config`, `delete_ai_provider_config` ve `run_ai_gitnexus_query` komutları eklendi. Provider kayıtları Tauri app data altında `settings.sqlite` içinde saklanır. OpenAI provider'ı Responses API, DeepSeek ve OpenAI-compatible provider'lar Chat Completions formatıyla çağrılır. AI workflow önce GitNexus query çalıştırır, sonra GitNexus çıktısını seçili modele özetletir.

Plan:

- SQLite tablo standardı: `ai_provider_configs(provider, api_key, model, base_url, updated_at)`.
- OpenAI preset: `gpt-5.5`, `gpt-5.4-mini`, `gpt-5.4-nano`.
- DeepSeek preset: varsayılan `deepseek-v4-flash`, ayrıca `deepseek-v4-pro`; uyumluluk alias'ları olarak `deepseek-chat`, `deepseek-reasoner`.
- OpenAI-compatible preset: kullanıcı base URL ve model override eder.
- API key alanı write-only davranır; listeleme komutu anahtarı döndürmez.
- AI yanıtı JSON olarak `provider`, `model`, `gitnexus_query`, `gitnexus_output`, `gitnexus_json`, `answer` alanlarını taşır.

Kabul kriterleri:

- Provider kaydedilmeden AI workflow çalıştırıldığında net hata döner.
- Provider kaydı sonrası aynı ekranda model/base URL/saved state güncellenir.
- GitNexus query başarısızsa LLM çağrısı yapılmaz ve hata kullanıcıya döner.

### BL-P1-009 - GitNexus Komut Sağlığı ve Abey Kalibrasyonu

Status: Tamamlandı

Tamamlanma Notu: `D:\PROGRAMMING\abey` için GitNexus index üretimi doğrulandı; 26.615 node, 44.124 edge, 799 cluster ve 300 flow üretildi. `gitnexus status` up-to-date dönüyor. CLI query'nin `-r` parametresinde path yerine repo label kullanılması gerektiği tespit edildi ve PyShield adapter bu davranışa göre düzeltildi. Query çıktısında FTS warning gibi sağlık sinyalleri frontend raw output ve graph inspector alanlarında görünür.

Plan:

- `run_gitnexus_analyze(folder, force)` Tauri komutu.
- `run_gitnexus_query(folder, query, limit)` Tauri komutu.
- Repo selector: target path'in son klasör adı GitNexus registry label olarak kullanılır.
- Force rebuild butonu FTS/query sağlığı bozulduğunda kullanılabilir.
- Abey gerçek proje kalibrasyonu bu akışla yapılır.

Kabul kriterleri:

- `abey` üzerinde GitNexus index üretimi frontend/backend akışından tetiklenebilir.
- Query limit kullanıcı tarafından seçilebilir.
- GitNexus command hataları uygulamayı çökertmez; raw output veya toast ile görünür.

### BL-P1-010 - Tool Bazlı Çalıştırma ve View Layout İyileştirmeleri

Status: Tamamlandı

Tamamlanma Notu: Üst command bar'daki Native/Full dropdown'u görünür segmented tool seçimine çevrildi. Kullanıcı artık `Native`, `Full`, `Ruff`, `Bandit`, `Pyright` veya `GitNexus` hedeflerinden birini seçip yalnız o aracı çalıştırabilir. Backend tarafında `AnalysisOptions` native analiz ve external tool listesini ayrı ayrı taşıyacak şekilde genişletildi; Tauri `run_analysis_on_folder` komutu `selected_tools` parametresi kabul ediyor. `tests/p0_regressions.rs` içinde yalnız Ruff çalıştırma regression testi eklendi.

UI Kapanış Notu: Issues ekranında sol tablo paneli viewport yüksekliğine stretch edildi ve uzun Pyright mesajları üç satırda clamp edilerek inspector'a taşındı. GitNexus ekranında graph canvas ve raw output paneli viewport içine sığacak grid düzenine alındı. AI Workbench ve GitNexus inspector içinde boş query + FTS indeks uyarısı özel callout olarak gösteriliyor; callout içinden `Force Rebuild` aksiyonu çalıştırılabiliyor.

Seçim/Inspector Düzeltme Notu: Overview `Selected Issue` ve Issues `Inspector` panelleri viewport içinde bağımsız scroll container olarak güncellendi. Issue seçim modeli uzun mesaj/path içeren HTML `data-*` key yerine kısa numeric id kullanıyor; Pyright gibi uzun mesajlı bulgularda satır seçimi artık iki inspector alanını da tutarlı doldurur.

GitNexus Rebuild Notu: PyShield GitNexus analyze komutu `--index-only` yerine `--skip-git --skip-agents-md --skip-skills` kullanacak şekilde değiştirildi. Böylece hedef repoya AGENTS/CLAUDE/skill dosyası enjekte edilmeden ve git repo zorunluluğuna takılmadan GitNexus'in normal FTS rebuild yolu tetiklenir. 1.6.5 stable sürümünde Abey query hâlâ `FTS indexes missing` uyarısı döndürebiliyor; UI bunu AI/API key hatasından ayrı bir GitNexus query sağlık sinyali olarak gösterir. Query boş + FTS warning durumunda backend otomatik `gitnexus cypher` keyword fallback çalıştırır ve bulunan node'ları `definitions` alanına ekler.

Kabul kriterleri:

- Kullanıcı yalnız Ruff/Bandit/Pyright/GitNexus çalıştırabilir.
- Native analiz seçilmediğinde PyShield status `not_run` görünür.
- Issues tablosu ekranın yarısında kalmaz; uzun mesajlar tablo satırını büyütmez.
- GitNexus raw output ekran dışında kalmaz.
- FTS warning durumunda kullanıcıya force rebuild aksiyonu gösterilir.

## P2 - CI Uyumu, AI Risk Katmanı ve Ölçeklenme

P2 maddeleri ürünün pazarlanabilir ve ileri seviye kullanım değerini artırır.

### BL-P2-001 - SARIF Export

Amaç: GitHub Code Scanning ve CI/CD ekosistemiyle uyum.

Yapılacaklar:

- Normalize Issue modelinden SARIF 2.1.0 üret.
- CLI opsiyonu ekle: `--sarif rapor.sarif`.
- Rule metadata alanlarını doldur.

Kabul kriterleri:

- Üretilen SARIF GitHub Code Scanning tarafından kabul edilir.

### BL-P2-002 - Semgrep Custom Rule Pack

Amaç: Framework-aware ve AI-generated risk kurallarını hızlı genişletmek.

Yapılacaklar:

- Semgrep adapter ekle.
- PyShield AI rule pack için ayrı klasör standardı belirle.
- İlk kurallar:
  - FastAPI endpoint içinde blocking call
  - Flask route return eksikliği
  - raw SQL string birleştirme
  - insecure temp file kullanımı

Kabul kriterleri:

- Custom Semgrep kuralı eklemek Rust kodu değiştirmeyi gerektirmez.

### BL-P2-003 - Risk Skoru ve Açıklanabilir Özet

Yapılacaklar:

- Dosya ve proje bazlı risk skoru üret:
  - Security Risk
  - AI Hallucination Risk
  - Dependency Risk
  - Maintainability Risk
- Skor hesabını rule severity, tool confidence ve kategori ağırlıklarıyla yap.

Kabul kriterleri:

- Dashboard sadece toplam hata değil, neden riskli olduğunu da açıklar.

### BL-P2-004 - AI Fix Önerisi ve Re-run Validation

Amaç: Düzeltme önerisi verilecekse doğrulamasız öneri üretmemek.

Yapılacaklar:

- Issue bazlı fix hint altyapısını genişlet.
- Opsiyonel AI fix önerisi üret.
- Fix sonrası Ruff/Bandit/Pyright/PyShield tekrar çalıştırılarak doğrulama sonucu göster.

Kabul kriterleri:

- Önerilen fix doğrulanmadan "çözüldü" sayılmaz.
- Kullanıcı kodun buluta gönderilip gönderilmeyeceğini açıkça kontrol eder.

### BL-P2-005 - Performans ve Büyük Repo Stratejisi

Yapılacaklar:

- Büyük repo taramalarında progress event gönder.
- Tauri UI'da gerçek zamanlı durum göster.
- Dosya hash cache ile değişmeyen dosyaları atlama opsiyonu tasarla.
- Analiz iptal etme desteği ekle.

Kabul kriterleri:

- Büyük klasör taramasında UI donmaz.
- Kullanıcı analizi iptal edebilir.

## Önerilen Uygulama Sırası

1. Sprint 1 - P0 şema, escaping, exclude ve fixture test altyapısı.
2. Sprint 2 - P0 import alias, attribute dangerous call ve scope iyileştirmeleri.
3. Sprint 3 - P1 AnalyzerAdapter iskeleti ve Ruff adapter.
4. Sprint 4 - P1 Bandit/Pyright adapterları ve Tauri filtreleri.
5. Sprint 5 - P2 SARIF, Semgrep rule pack ve risk skoru.

## Definition of Done

- `cargo test` başarılı.
- Yeni kurallar fixture regression testiyle sabitlenmiş.
- JSON, HTML ve Tauri UI aynı normalize Issue modelini kullanıyor.
- Kullanıcı kontrollü metinler HTML/DOM tarafında escape ediliyor.
- Harici tool eksikliği uygulamayı çökertmiyor.
- README, yeni komutlar ve ürün konumlandırmasıyla güncellenmiş.
