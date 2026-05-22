# Tehlikeli çağrılar - güvenlik riskli
result1 = eval("1 + 1")
result2 = exec("print('hello')")

# os.system çağrısı
import os
status = os.system("ls -la")

# Güvenli çağrılar
print("Safe code")
print(len([1, 2, 3]))
