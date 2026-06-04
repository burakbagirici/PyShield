import os
import subprocess


eval("1 + 1")
exec("print('hello')")
system("ls -la")
os.system("ls -la")
subprocess.run("ls -la", shell=True)
subprocess.Popen(["ls", "-la"])
subprocess.call(["ls", "-la"])
subprocess.check_output(["ls", "-la"])

print("safe")
len([1, 2, 3])
