import sys

def hesapla(a, b):
    return a + b

def main():
    # Doğru fonksiyon çağrısı
    sonuc = hesapla(10, 20)
    print(sonuc)
    
    # Halüsinasyon: Tanımlanmamış fonksiyon
    deger = mysterious_function(5)
    
    # Tanımlanmamış değişken
    print(undefined_variable)
    
    # Eksik import
    import_kullan = math.sqrt(16)
    
    # For loop test
    for i in range(5):
        print(i)

main()
