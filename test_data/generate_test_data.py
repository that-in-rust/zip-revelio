#!/usr/bin/env python3

import os
import zipfile
import random
import string

def generate_random_text(size):
    """Generate random text of specified size."""
    return ''.join(random.choices(string.ascii_letters + string.digits + '\n', k=size))

def generate_random_binary(size):
    """Generate random binary data of specified size."""
    return bytes(random.getrandbits(8) for _ in range(size))

def create_test_zip(filename, total_size):
    """Create a test ZIP file with mixed content."""
    with zipfile.ZipFile(filename, 'w') as zf:
        current_size = 0
        file_count = 0
        
        while current_size < total_size:
            # Decide file type and size
            is_text = random.choice([True, False])
            file_size = random.randint(1024, min(total_size // 10, 1024 * 1024))
            
            # Generate content
            if is_text:
                content = generate_random_text(file_size).encode('utf-8')
                ext = random.choice(['.txt', '.md', '.log', '.csv'])
            else:
                content = generate_random_binary(file_size)
                ext = random.choice(['.bin', '.dat', '.img'])
            
            # Create file
            filename = f'file_{file_count}{ext}'
            compression = random.choice([zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED])
            zf.writestr(zipfile.ZipInfo(filename), content, compression)
            
            current_size += file_size
            file_count += 1

def create_corrupted_zip():
    """Create an intentionally corrupted ZIP file."""
    with open('corrupted.zip', 'wb') as f:
        # Write valid ZIP header
        f.write(b'PK\x03\x04')
        # Write corrupted data
        f.write(os.urandom(1024))

def main():
    # Create test files
    create_test_zip('1mb.zip', 1024 * 1024)  # 1MB
    create_test_zip('10mb.zip', 10 * 1024 * 1024)  # 10MB
    create_corrupted_zip()

if __name__ == '__main__':
    main()
