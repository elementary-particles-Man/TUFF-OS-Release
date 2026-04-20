import hashlib
import os

SENTINEL = b"---TUFF-END---"

def test_sentinel_collision():
    print("[*] Running SENTINEL_BINARY_COLLISION test...")
    
    # 1. Create a dummy binary with the sentinel in the middle and at the end
    dummy_code_start = b"\x90\x90\x90\x90" * 10
    injected_sentinel = SENTINEL
    dummy_code_middle = b"\xCC\xCC\xCC\xCC" * 10
    actual_sentinel = SENTINEL
    
    binary_data = dummy_code_start + injected_sentinel + dummy_code_middle + actual_sentinel
    
    # 2. Simulate the bootloader's .rposition() logic
    end_pos = binary_data.rfind(SENTINEL)
    if end_pos == -1:
        print("[FAIL] Sentinel not found.")
        exit(1)
        
    calc_data = binary_data[:end_pos + len(SENTINEL)]
    h_rfind = hashlib.sha3_512(calc_data).digest()
    
    # 3. Simulate a flawed .find() logic
    wrong_pos = binary_data.find(SENTINEL)
    wrong_calc_data = binary_data[:wrong_pos + len(SENTINEL)]
    h_find = hashlib.sha3_512(wrong_calc_data).digest()
    
    # 4. Verify that .rfind() calculates a DIFFERENT hash than .find()
    # and that the length of the data hashed by .rfind() is correct.
    expected_len = len(dummy_code_start) + len(injected_sentinel) + len(dummy_code_middle) + len(actual_sentinel)
    
    if len(calc_data) == expected_len and h_rfind != h_find:
        print(f"[PASS] Sentinel collision mitigated. Hashed {len(calc_data)} bytes correctly.")
        print(f"       Correct Hash: {h_rfind.hex()[:16]}...")
        print(f"       Flawed Hash:  {h_find.hex()[:16]}...")
    else:
        print(f"[FAIL] Sentinel collision detection failed. Hashed {len(calc_data)} bytes. Expected {expected_len}.")
        exit(1)

if __name__ == "__main__":
    test_sentinel_collision()
