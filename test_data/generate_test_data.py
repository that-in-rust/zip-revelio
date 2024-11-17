#!/usr/bin/env python3
"""
Test Data Generator for ZIP-Revelio

This script generates test data files with various characteristics:
- Different sizes (1MB, 10MB)
- Various compression methods
- Corrupted files for error testing
"""

import os
import random
import zipfile
from pathlib import Path

class TestDataGenerator:
    def __init__(self, output_dir: str):
        self.output_dir = Path(output_dir).absolute()
        if not self.output_dir.exists():
            self.output_dir.mkdir(parents=True)
        
    def generate_standard_files(self):
        """Generate 1MB and 10MB test files"""
        self.generate_zip_file("1mb.zip", 1024 * 1024)
        self.generate_zip_file("10mb.zip", 10 * 1024 * 1024)
        
    def generate_corrupted_file(self):
        """Generate corrupted ZIP for error testing"""
        path = self.output_dir / "corrupted.zip"
        with path.open("wb") as f:
            f.write(b"PK\x03\x04")  # Valid signature
            f.write(os.urandom(1024))  # Random corrupt data
            
    def generate_zip_file(self, name: str, size: int):
        """Generate ZIP file with specified size"""
        path = self.output_dir / name
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
            # Create a directory structure
            directories = ["docs", "data", "images", "config"]
            for directory in directories:
                remaining = size // len(directories)
                file_count = 0
                
                while remaining > 0:
                    # Vary chunk sizes to make it more realistic
                    chunk_size = min(remaining, random.randint(1024, 64 * 1024))
                    data = os.urandom(chunk_size)
                    
                    # Add some variety to filenames and extensions
                    ext = random.choice([".txt", ".dat", ".bin", ".log"])
                    filename = f"{directory}/file_{file_count:04d}{ext}"
                    
                    # Alternate between STORED and DEFLATED
                    compression = zipfile.ZIP_DEFLATED if file_count % 2 == 0 else zipfile.ZIP_STORED
                    zf.writestr(zipfile.ZipInfo(filename), data, compress_type=compression)
                    
                    remaining -= chunk_size
                    file_count += 1

if __name__ == "__main__":
    current_dir = Path(__file__).parent.absolute()
    generator = TestDataGenerator(current_dir)
    generator.generate_standard_files()
    generator.generate_corrupted_file()
    print(f"Generated test files in {current_dir}")
