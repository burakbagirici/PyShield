# Tehlikeli çağrılar - güvenlik riskli
result1 = eval("1 + 1")
result2 = exec("print('hello')")
os_result = system("ls -la")

# Güvenli çağrılar
print("Safe code")
print(len([1, 2, 3]))
