# Test Data for ZIP-Revelio

This directory contains test ZIP files of various sizes and structures for testing and benchmarking ZIP-Revelio.

## Test Files

1. `1mb.zip` - 1MB test file containing:
   - Text files
   - Small images
   - Mixed compression methods

2. `10mb.zip` - 10MB test file containing:
   - Larger text files
   - Images and documents
   - Mixed compression ratios

3. `corrupted.zip` - Intentionally corrupted ZIP for error handling tests

## Generation Scripts

Use the provided Python script to generate test data:

```bash
python3 generate_test_data.py
```
