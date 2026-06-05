#!/usr/bin/env python3
"""
Production-ready SAST test file - False Positive Prevention
Yanlış Pozitif (False Positive) Tespitini Test Etme
"""

import argparse
import os
import sys

# Test 1: __name__, __file__, __doc__ - Bu satırlar uyarı vermemeli
if __name__ == '__main__':
    print(f"Script file: {__file__}")
    print(f"Module name: {__name__}")
    print(f"Module docstring: {__doc__}")

# Test 2: Keyword arguments - 'description' ve 'metavar' uyarı vermemeli
parser = argparse.ArgumentParser(
    description="AI Kod Analiz Aracı",
    formatter_class=argparse.RawDescriptionHelpFormatter,
    epilog="Örnek kullanım:\n  %(prog)s path/to/python/files"
)

# Keyword argument parametreleri - bunlar uyarı vermemeli
parser.add_argument(
    'input_path',
    metavar='FILE_OR_DIR',
    help='Analiz edilecek Python dosyası veya klasörü',
    type=str
)

parser.add_argument(
    '--output',
    dest='output_file',
    default='rapor.json',
    action='store',
    type=str,
    help='Çıktı dosyası adı'
)

# Test 3: self ve cls scope değişkenleri - uyarı vermemeli
class CodeAnalyzer:
    """AI-powered code analyzer using Tree-sitter"""
    
    def __init__(self, name):
        self.name = name
        self.results = []
    
    def analyze(self, code):
        """Analyze code"""
        return self.process_code(code)
    
    def process_code(self, source):
        """Process the source code"""
        self.results.append(source)
        return len(self.results)
    
    @staticmethod
    @classmethod
    @property
    def static_info(cls):
        """Static method should skip decorator analysis"""
        return cls.__name__

# Test 4: Wildcard import yönetimi - from module import * ile hata vermemeli
def with_wildcard():
    """This would normally fail if wildcard import is there"""
    from pathlib import *
    # Wildcard'dan gelen fonksiyonlar hata vermemeli
    p = Path(".")
    return p.exists()

# Test 5: self.attribute ve cls.attribute - dinamik scope yönetimi
class DataProcessor:
    config = {'debug': True}
    
    def process(self, data):
        """Process data using instance attributes"""
        self.processed = data.upper()
        self.count = len(data)
        return self.processed
    
    def info(self):
        """Get info using class attributes"""
        return self.config

# Test 6: Built-in istisnalar - uyarı vermemeli
def handle_errors():
    try:
        result = int("abc")
    except ValueError as e:
        print(f"ValueError: {e}")
    except TypeError:
        print("TypeError occurred")
    except (KeyError, IndexError) as err:
        print(f"Dict/List error: {err}")
    except Exception:
        print("Generic exception")
    finally:
        print("Cleanup")

# Test 7: Built-in fonksiyonları - uyarı vermemeli
def builtin_operations():
    """Test all built-in functions"""
    numbers = [1, 2, 3, 4, 5]
    
    # Built-in işlemler
    total = sum(numbers)
    maximum = max(numbers)
    minimum = min(numbers)
    
    squares = list(map(lambda x: x**2, numbers))
    evens = list(filter(lambda x: x % 2 == 0, numbers))
    
    sorted_nums = sorted(numbers, reverse=True)
    reversed_nums = list(reversed(numbers))
    
    enumerated = list(enumerate(numbers))
    zipped = list(zip(numbers, numbers))
    
    # Type checking
    is_list = isinstance(numbers, list)
    is_type = type(numbers)
    
    # Attribute access
    attributes = dir(numbers)
    
    return {
        'sum': total,
        'max': maximum,
        'min': minimum,
        'squares': squares,
        'evens': evens,
        'sorted': sorted_nums,
    }

# Test 8: Magic attributes - uyarı vermemeli
def inspect_module():
    """Inspect magic attributes"""
    module_name = __name__
    module_file = __file__
    module_doc = __doc__
    
    # Package info
    package = __package__
    loader = __loader__
    spec = __spec__
    cached = __cached__
    
    return {
        'name': module_name,
        'file': module_file,
        'doc': module_doc,
        'package': package,
        'loader': loader,
        'spec': spec,
        'cached': cached,
    }

# Test 9: Decorator context - uyarı vermemeli
def decorator_factory(func):
    """Factory for creating decorators"""
    def wrapper(*args, **kwargs):
        return func(*args, **kwargs)
    return wrapper

@decorator_factory
@staticmethod
@classmethod
@property
def decorated_function():
    """This is decorated and should skip decorator analysis"""
    return "decorated"

# Test 10: Comprehension scoping - uyarı vermemeli
def comprehensions():
    """Test list/dict/set comprehensions"""
    numbers = range(10)
    
    # List comprehension - 'x' and 'y' are local to comprehension
    squares = [x**2 for x in numbers]
    filtered = [x for x in numbers if x > 5]
    
    # Dict comprehension
    dict_comp = {x: x**2 for x in numbers}
    
    # Set comprehension
    set_comp = {x % 3 for x in numbers}
    
    # Generator expression
    gen = (x*2 for x in numbers)
    
    # Nested comprehensions
    matrix = [[y for y in range(3)] for x in range(3)]
    
    return {
        'squares': squares,
        'filtered': filtered,
        'dict': dict_comp,
        'set': set_comp,
        'gen': list(gen),
        'matrix': matrix,
    }

if __name__ == '__main__':
    # Bu scope altında __name__ == '__main__' doğru olduğu için false positive vermemeli
    analyzer = CodeAnalyzer(__name__)
    print(analyzer.analyze("test code"))
    print(builtin_operations())
    print(handle_errors())
    print(comprehensions())
