# =============================================================
# AI Kod Analiz Motoru - B2B SaaS Kapsamlı Test Dosyası
# Tetiklenen Kurallar:
#   Kural 1  : Sözdizimi hatası YOK (temiz dosya)
#   Kural 2  : Fonksiyon Halüsinasyonu   -> olmayan_fonksiyon()
#   Kural 3  : Eksik Import              -> json.dumps() (json import edilmemiş)
#   Kural 4  : Tanımlanmamış Değişken    -> tanimsiz_degisken
#   Kural 5  : Kullanılmayan Kod         -> import os, import re, bos_degisken
#   Kural 6  : Güvenlik İhlali           -> api_key, db_password
#   Kural 7  : Sonsuz Döngü Riski        -> while True (break yok)
# =============================================================

# --- KULLANILMAYAN IMPORT'LAR (Kural 5) ---
import os
import re

# --- KULLANILAN IMPORT (sorun yok) ---
import math

# --- KULLANILMAYAN DEĞİŞKEN (Kural 5) ---
bos_degisken = 99

# --- KULLANILAN DEĞİŞKEN (sorun yok) ---
pi_degeri = math.pi

# --- GÜVENLİK İHLALLERİ (Kural 6) ---
api_key = "sk-proj-abc123def456ghi789jkl012mno345pqr678" # noqa
db_password = "super_secret_production_pass!"
normal_metin = "Bu normal bir stringdir"


def topla(a, b):
    """İki sayıyı toplar."""
    return a + b


def hesapla():
    sonuc = topla(3, 4)

    # Kural 2: Halüsinasyon
    olmayan_sonuc = olmayan_fonksiyon(sonuc) # noqa

    # Kural 3: Eksik import
    veri = json.dumps({"sonuc": olmayan_sonuc})

    # Kural 4: Tanımlanmamış değişken
    toplam = sonuc + tanimsiz_degisken

    print(toplam)
    print(pi_degeri)
    return veri


# --- SONSUZ DÖNGÜ RİSKİ (Kural 7) ---
# Bu döngüde break/return yok -> tehlikeli!
def tehlikeli_dongu():
    while True:
        print("Bu döngü asla durmaz!")

# Bu döngü güvenli -> break var
def guvenli_dongu():
    while True:
        veri = input("Çıkmak için 'q' yazın: ")
        if veri == "q":
            break


hesapla()