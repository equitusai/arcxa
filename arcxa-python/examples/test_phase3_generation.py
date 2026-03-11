#!/usr/bin/env python3
"""Quick test of Phase 3 parallel data generation"""
import sys
import time

# Add parent to path
sys.path.insert(0, '..')

from healthcare_etl_demo_v7 import create_healthcare_data_with_tracking

print("=" * 70)
print("Phase 3 Parallel Data Generation Test")
print("=" * 70)
print()

# Test with 200K records
csv_path, expected_unique, tracker = create_healthcare_data_with_tracking(200000)

print()
print("=" * 70)
print("Test Results:")
print(f"  CSV File: {csv_path}")
print(f"  Expected Unique: {expected_unique}")
print(f"  Duplicate Groups: {tracker.summary()['duplicate_groups']}")
print("=" * 70)
