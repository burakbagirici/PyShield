import subprocess
import tempfile
import os
import re
import sys

# ╔══════════════════════════════════════════════════════════════════╗
# ║              🛡️ PyShield.ai — AI Ajan Entegrasyonu               ║
# ║                                                                  ║
# ║  Bu script, bir şirketin kendi AI ajanının PyShield.ai motorunu  ║
# ║  arka planda nasıl tetikleyip sonuçlarına göre aksiyon aldığını  ║
# ║  gösterir. Gerçek dünya B2B kullanım senaryosunun simülasyonu.   ║
# ╚══════════════════════════════════════════════════════════════════╝

PYSHIELD_BINARY = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "target", "debug", "ai_code_analyzer.exe" if os.name == 'nt' else "ai_code_analyzer"
)

MAX_RETRY = 3

def ai_agent_generate_code(attempt: int) -> str:
    if attempt == 1:
        return '''import os
import re

api_key = "sk-live-abc123def456ghi789jkl012mno345pqr678"
db_password = "production_db_p@ssw0rd!"

def analiz_yap(veri):
    sonuc = veri_isle(veri)
    rapor = json.dumps({"sonuc": sonuc})
    return rapor + bilinmeyen_degisken

def sunucu_dinle():
    while True:
        print("Bağlantı bekleniyor...")

analiz_yap([1, 2, 3])
'''
    elif attempt == 2:
        return '''import os
import json

def analiz_yap(veri):
    sonuc = veri_isle(veri)
    rapor = json.dumps({"sonuc": sonuc})
    return rapor

def sunucu_dinle():
    while True:
        data = input("Komut: ")
        if data == "quit":
            break

analiz_yap([1, 2, 3])
'''
    else:
        return '''import json

def veri_isle(veri):
    return [x * 2 for x in veri]

def analiz_yap(veri):
    sonuc = veri_isle(veri)
    rapor = json.dumps({"sonuc": sonuc})
    return rapor

print(analiz_yap([1, 2, 3]))
'''

def run_pyshield(code: str) -> dict:
    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False, encoding="utf-8") as tmp:
        tmp.write(code)
        tmp_path = tmp.name

    try:
        result = subprocess.run(
            [PYSHIELD_BINARY, tmp_path],
            capture_output=True, text=True, encoding="utf-8", errors="replace"
        )
        output = result.stdout + result.stderr

        total_issues = 0
        summary_lines = []
        in_summary = False

        for line in output.splitlines():
            if "GENEL ÖZET" in line:
                in_summary = True
            if in_summary:
                summary_lines.append(line)
            match = re.search(r"Toplam Sorun\s*:\s*(\d+)", line)
            if match:
                total_issues = int(match.group(1))

        return {
            "raw_output": output,
            "total_issues": total_issues,
            "passed": total_issues == 0,
            "summary_lines": summary_lines,
        }
    finally:
        os.unlink(tmp_path)

def main():
    print("=" * 66)
    print("🤖 AI Ajan Simülasyonu Başlatılıyor...")
    print("🛡️  PyShield.ai Güvenlik Kalkanı Aktif")
    print("=" * 66)

    if not os.path.exists(PYSHIELD_BINARY):
        print(f"\n❌ PyShield binary bulunamadı: {PYSHIELD_BINARY}\nÖnce 'cargo build' çalıştırın.")
        sys.exit(1)

    for attempt in range(1, MAX_RETRY + 1):
        print(f"\n{'─' * 66}\n📝 Deneme {attempt}/{MAX_RETRY}: AI ajanı kod üretiyor...\n{'─' * 66}")
        generated_code = ai_agent_generate_code(attempt)
        
        print("\n🔍 PyShield.ai taraması başlıyor...")
        scan_result = run_pyshield(generated_code)

        if scan_result["passed"]:
            print("\n" + "=" * 66 + "\n✅ BAŞARILI! Kod tüm güvenlik kontrollerinden geçti.\n🚀 Kod üretime alınabilir (deploy-ready).\n" + "=" * 66)
            break
        else:
            print(f"\n🚨 AI Ajanı Uyarısı: Kodda {scan_result['total_issues']} kritik hata tespit edildi, kod canlıya alınamaz!")
            if scan_result["summary_lines"]:
                for line in scan_result["summary_lines"]:
                    print(f"      {line}")

if __name__ == "__main__":
    main()