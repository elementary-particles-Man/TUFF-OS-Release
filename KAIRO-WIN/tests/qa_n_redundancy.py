import os
import subprocess

def test_n_redundancy():
    print("[*] Running N_REDUNDANCY_RECOVERY test...")
    
    # 1. Create 3 valid aha_copy_N.bin files
    data = b"VALID_N_REDUNDANCY_DATA"
    for i in range(3):
        with open(f"aha_copy_{i}.bin", "wb") as f:
            f.write(data)
            
    # 2. Delete one to simulate physical detachment
    os.remove("aha_copy_1.bin")
    print("[*] Simulated physical detachment of disk 1 (aha_copy_1.bin removed).")
    
    # 3. Run tuffutl fs audit-n
    tuffutl_path = "/media/flux/THPDOC/Develop/TUFF-OS/TUFF-UTL/target/debug/tuffutl"
    result = subprocess.run([tuffutl_path, "fs", "audit-n"], capture_output=True, text=True)
    
    if "Majority Vote Status: STABLE" in result.stdout:
        print("[PASS] N_REDUNDANCY_RECOVERY: System remained stable with 2/3 members.")
    else:
        print("[FAIL] N_REDUNDANCY_RECOVERY: System failed to recover from disk loss.")
        print(result.stdout)
        exit(1)

if __name__ == "__main__":
    test_n_redundancy()
