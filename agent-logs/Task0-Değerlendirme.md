İnceledim. Bu proje temelde **Rust ile yazılmış, Python dosyalarını statik olarak analiz eden bir “AI code analyzer / PyShield” motoru**. Yani kendisi Python kodu çalıştırmıyor; `.py` dosyalarını okuyup Tree-sitter ile AST’ye çeviriyor, sonra belli kurallarla hata/şüphe/risk raporu üretiyor.

## 1. Dosyaların görevi

### `main.rs`

Burası komut satırı giriş noktası. Program şu şekilde çalışıyor:

```bash
cargo run -- ./analiz_edilecek_klasor
```

Akış şöyle:

1. Kullanıcıdan klasör yolu alıyor.
2. Yol geçerli klasör mü kontrol ediyor.
3. `analyze_folder(path_arg)` fonksiyonunu çağırıyor.
4. Sonuçları terminale yazdırıyor.
5. `generate_html_report(&report)` ile `rapor.html` oluşturuyor.
6. Ayrıca `rapor.json` yazıyor.
7. Hata varsa `exit code 1`, hata yoksa `exit code 0` ile kapanıyor. 

Bu taraf genel olarak temiz ve anlaşılır.

---

### `parser.rs`

Bu dosya sadece Python kodunu Tree-sitter ile parse ediyor.

```rust
pub fn parse_python_code(source_code: &str) -> Option<Tree>
```

İçeride:

1. `Parser::new()` oluşturuluyor.
2. `tree_sitter_python::LANGUAGE` parser’a veriliyor.
3. Python kaynak kodundan AST ağacı üretiliyor. 

Bu dosya basit, doğru ve amacına uygun.

---

### `lib.rs`

Burası projenin ana motoru. Şunları yapıyor:

1. Verilen klasörü recursive tarıyor.
2. Sadece `.py` dosyalarını topluyor.
3. Her Python dosyasını analiz ediyor.
4. Analiz sonuçlarını `Issue`, `FileAnalysisResult`, `AnalysisReport` gibi struct’lara koyuyor.
5. Rayon ile dosyaları paralel analiz ediyor.
6. JSON ve HTML rapor için veri hazırlıyor.

Önemli nokta: `# noqa` veya `# analiz-atla` geçen satırlardaki uyarıları filtreliyor. Bu iyi düşünülmüş bir özellik. 

Analiz sırası kabaca şöyle:

```text
Klasör seçilir
    ↓
.py dosyaları bulunur
    ↓
Her dosya okunur
    ↓
Tree-sitter ile AST üretilir
    ↓
Önce syntax error aranır
    ↓
Syntax error yoksa diğer kurallar çalışır:
    - fonksiyon halüsinasyonu
    - eksik import
    - tanımsız değişken
    - kullanılmayan kod
    - hardcoded secret
    - eval / exec / system
    - sonsuz döngü riski
    ↓
terminal + rapor.json + rapor.html
```

---

### `rules.rs`

Burası projenin “zeka” kısmı. Tree-sitter query’leriyle Python AST içinde örüntü arıyor.

Şu kurallar var:

* Syntax error tespiti
* Çağrılan fonksiyonları toplama
* Tanımlı fonksiyonları bulma
* Import’ları bulma
* Tanımsız değişken tespiti
* Kullanılmayan import/değişken tespiti
* Eksik import tespiti
* Hardcoded secret/API key tespiti
* `while True` / `while 1` sonsuz döngü riski
* `eval`, `exec`, `system` gibi tehlikeli çağrılar

Ayrıca `OnceLock<Query>` kullanılmış; yani Tree-sitter query’leri her analizde yeniden derlenmiyor. Bu performans açısından doğru bir karar. 

## 2. Proje güçlü mü?

Evet, başlangıç için güçlü. Özellikle şu tarafları iyi:

**Tree-sitter kullanımı doğru tercih.** Regex ile Python kodu analiz etmeye çalışmak çok hatalı olurdu. Burada AST tabanlı yaklaşım seçilmiş.

**Paralel analiz var.** `.py` dosyaları Rayon ile paralel taranıyor. Büyük klasörlerde ciddi hız kazandırır. 

**Rapor formatı iyi düşünülmüş.** Hem terminal çıktısı, hem JSON, hem HTML dashboard var. Bu projeyi CLI aracından CI/CD aracına çevirmek kolay olur.

**Satır bazlı muafiyet var.** `# noqa` ve `# analiz-atla` desteği pratikte çok işe yarar. 

**Wildcard import farkındalığı var.** `from x import *` varsa bazı bulguları direkt ERROR yerine WARNING gibi düşünmeye çalışıyor. Bu güzel bir niyet.

## 3. En önemli eksikler / riskler

### 1. `Cargo.toml` dosyası yok

Yüklenenler arasında `Cargo.toml` görünmüyor. Bu haliyle proje tek başına derlenemez. En azından şu bağımlılıklar gerekir:

```toml
[package]
name = "ai_code_analyzer"
version = "0.1.0"
edition = "2021"

[dependencies]
tree-sitter = "0.24"
tree-sitter-python = "0.23"
streaming-iterator = "0.1"
rayon = "1.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
```

Sürümler ortamına göre değişebilir ama mantık bu.

---

### 2. “Fonksiyon halüsinasyonu” kuralı fazla agresif

Şu an sistem çağrılan fonksiyonu sadece şu iki şeyle karşılaştırıyor:

* dosyada tanımlı fonksiyonlar
* Python built-in fonksiyonları

Ama import edilen fonksiyonları tam hesaba katmıyor. Mesela Python’da şu kod geçerli:

```python
from math import sqrt

print(sqrt(16))
```

Bu projede `sqrt` muhtemelen “halüsinasyon fonksiyonu” gibi işaretlenebilir. Çünkü `sqrt` dosyada tanımlı değil ve built-in değil. Halbuki import edilmiş.

Daha doğru yaklaşım:

```text
defined_functions
+ imported_symbols
+ assigned_callables
+ class methods
+ local scope symbols
+ builtins
```

birlikte değerlendirilmelidir.

---

### 3. `math.sqrt()` gibi attribute call’lar eksik kalabilir

`collect_all_function_calls` sorgusu sadece şu pattern’i yakalıyor:

```scheme
(call function: (identifier) @func_name)
```

Yani `foo()` yakalanır ama `math.sqrt()` gibi çağrılarda function node bir `attribute` olur. Bu nedenle bazı gerçek çağrılar halüsinasyon analizine hiç girmeyebilir. 

Bunun için ikinci bir query eklenmeli:

```scheme
(call function: (attribute object: (identifier) @obj attribute: (identifier) @method))
```

---

### 4. Python scope analizi yüzeysel

Tanımsız değişken analizi iyi niyetli ama Python için scope oldukça karmaşıktır:

* nested function
* closure
* global / nonlocal
* class scope
* instance attribute
* `with open(...) as f`
* tuple unpacking
* destructuring assignment
* import alias
* `try/except` dalları
* pattern matching
* comprehensions
* walrus operator `:=`

Kod bazı false-positive’leri azaltmaya çalışıyor; comprehension, decorator, keyword argument, self/cls gibi bazı istisnalar var. Fakat hâlâ gerçek bir linter seviyesinde scope çözümlemesi yapmıyor. 

Örneğin şunlar eksik kalabilir:

```python
with open("a.txt") as f:
    data = f.read()
```

```python
a, b = get_values()
```

```python
import numpy as np
np.array([1, 2, 3])
```

Alias desteği özellikle önemli.

---

### 5. Import analizi alias’ları düzgün ele almıyor

Şu Python örneği çok yaygın:

```python
import numpy as np
import pandas as pd
from pathlib import Path as P
```

Mevcut query yapısı daha çok düz `import os`, `from os import path` gibi örneklere odaklı. Alias’lar doğru modellenmezse hem “eksik import”, hem “tanımsız değişken”, hem “kullanılmayan import” tarafında yanlış sonuçlar çıkar.

Bu proje için kritik ekleme:

```text
import numpy as np  → np tanımlı kabul edilmeli
from pathlib import Path as P → P tanımlı kabul edilmeli
```

---

### 6. Kullanılmayan kod tespiti basit seviyede

`find_unused_code` import ve assignment sol taraflarını “tanım” gibi görüyor, sonra kullanılan identifier listesiyle kıyaslıyor. Bu başlangıç için iyi ama şu durumları kaçırabilir:

```python
x += 1
```

```python
a, b = func()
```

```python
self.value = 10
```

```python
for key, value in items:
    ...
```

Ayrıca fonksiyon parametrelerinin kullanılmama durumu ayrı bir kategori olarak yok. Büyük projelerde en çok istenen analizlerden biri budur.

---

### 7. Güvenlik analizi sınırlı

Hardcoded secret tespiti sadece şu tarzı yakalıyor:

```python
api_key = "sk-..."
password = "..."
token = "..."
```

Bu iyi ama sınırlı. Şunlar eksik:

```python
config["api_key"] = "..."
os.environ["TOKEN"] = "..."
headers = {"Authorization": "Bearer ..."}
```

Ayrıca entropy tabanlı secret detection yok. Gerçek araçlarda sadece prefix değil, string’in rastgelelik/uzunluk yapısı da incelenir.

---

### 8. `os.system()` tespiti eksik olabilir

Kod `eval`, `exec`, `system` için identifier call arıyor. Bu şu çağrıları yakalar:

```python
eval("...")
exec("...")
system("...")
```

Ama genelde Python’da tehlikeli kullanım şöyle olur:

```python
import os
os.system("rm -rf /")
```

Burada çağrı `system` identifier olarak değil, `os.system` attribute call olarak gelir. Mevcut kural bunu kaçırabilir. 

Daha doğru güvenlik query’si:

```scheme
(call function: (attribute object: (identifier) @obj attribute: (identifier) @method))
```

Sonra:

```text
obj == "os" && method == "system"
obj == "subprocess" && method in ["run", "Popen", "call"]
```

gibi kontrol edilmeli.

---

### 9. Syntax error varsa diğer analizler çalışmıyor

`lib.rs` içinde syntax error bulunduğunda kod diğer kurallara geçmiyor. Bu anlaşılır ama pratikte bazen syntax error olan dosyada bile import/güvenlik/secret analizi yapılabilir.

Daha iyi yaklaşım:

```text
Syntax error varsa:
- syntax error raporla
- ama mümkün olan yüzeysel kontrolleri yine çalıştır
```

Tree-sitter hata düğümleri olsa bile kısmi AST üretebilir.

---

### 10. HTML raporda güvenlik ve escaping konusu

`output_buffer` için `<` ve `>` escape edilmiş. Bu iyi. Ama `project_path`, `file_path`, `analysis_timestamp` gibi alanlar doğrudan HTML içine basılıyor gibi görünüyor. Kullanıcı kontrollü path’ler teorik olarak HTML injection riski oluşturabilir. `generate_html_report` içinde tüm dinamik alanlar HTML escape edilmelidir. 

---

## 4. Bu araç şu an ne için uygun?

Şu anki haliyle bence bu proje:

* AI tarafından üretilen küçük/orta Python scriptlerini hızlı kontrol etmek,
* eğitim amaçlı kod analizi göstermek,
* “potansiyel sorun var mı?” diye ön tarama yapmak,
* JSON/HTML rapor üretmek,
* CI/CD’ye ilk seviye kalite kapısı olarak bağlamak

için uygun.

Ama henüz:

* Ruff,
* Pyright,
* Pylint,
* Bandit,
* Semgrep

seviyesinde güvenilir bir Python statik analiz motoru değil.

Bunu “kesin doğruluk veren linter” gibi değil, **AI kodu için hızlı risk tarayıcı** gibi konumlandırmak daha doğru olur.

## 5. Öncelikli geliştirme listesi

Bence sırayla şunları yapmalısın:

1. **Cargo.toml ekle.**  -- BU ZATEN VAR
2. `README.md` yaz: kullanım, çıktı dosyaları, örnek komutlar. -- BU ZATEN VAR
3. Import alias desteği ekle:

   * `import numpy as np`
   * `from x import y as z`
4. Fonksiyon halüsinasyonu analizine import edilen sembolleri dahil et.
5. Attribute call desteği ekle:

   * `math.sqrt()`
   * `os.system()`
   * `subprocess.run()`
6. `with ... as ...`, tuple unpacking, for unpacking desteği ekle.
7. HTML escaping’i merkezi fonksiyona bağla.
8. `--json`, `--html`, `--fail-on warning/error`, `--exclude venv` gibi CLI opsiyonları ekle.
9. `.gitignore`, `target/`, `.venv/`, `__pycache__`, `site-packages` gibi klasörleri tarama dışı bırak.
10. Test sayısını artır: false-positive testleri bu projede çok önemli.

## 6. Kısa sonuç

Bu proje fikri doğru yönde. Mimari olarak:

```text
main.rs  → CLI
lib.rs   → analiz orkestrasyonu + raporlama
parser.rs → Python AST üretimi
rules.rs → analiz kuralları
```

şeklinde temiz ayrılmış.

En büyük problem Rust tarafı değil; **Python semantiğinin fazla basitleştirilmiş olması**. Yani proje çalışır hale geldiğinde rapor üretebilir, fakat özellikle import, alias, attribute call, scope ve güvenlik kurallarında false-positive/false-negative üretir.

Benim net değerlendirmem:
**Bu haliyle iyi bir prototip. Üzerine 5-10 kritik kural düzeltmesi eklenirse ciddi, hızlı ve kullanışlı bir “AI Python Kod Risk Analizörü”ne dönüşebilir.**
