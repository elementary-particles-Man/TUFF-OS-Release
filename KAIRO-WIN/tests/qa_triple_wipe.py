#!/usr/bin/env python3
import os

DUMMY_FILE = "/tmp/tuff_wipe_test.bin"
LBA = 0
OFFSET = LBA * 512

def create_dummy():
    with open(DUMMY_FILE, "wb") as f:
        f.write(b"A" * 1024)

def simulate_triple_wipe():
    print("[*] Running TRIPLE_WIPE_FORENSIC_CHECK...")
    
    with open(DUMMY_FILE, "r+b") as f:
        # Pass 1: 0xFF
        f.seek(OFFSET)
        f.write(b"\xFF" * 512)
        f.flush()
        
        # Pass 2: 0x00
        f.seek(OFFSET)
        f.write(b"\x00" * 512)
        f.flush()
        
        # Pass 3: Random-ish (simulating the Rust logic)
        f.seek(OFFSET)
        buf_rnd = bytearray(512)
        for i in range(512):
            buf_rnd[i] = ((OFFSET + i) % 255) & 0xFF
        f.write(buf_rnd)
        f.flush()

def forensic_check():
    with open(DUMMY_FILE, "rb") as f:
        f.seek(OFFSET)
        data = f.read(512)
        
        if b"A" * 10 in data:
            print("[FAIL] Original data residue found.")
            return
        if data == b"\xFF" * 512:
            print("[FAIL] Stopped at Pass 1 (0xFF).")
            return
        if data == b"\x00" * 512:
            print("[FAIL] Stopped at Pass 2 (0x00).")
            return
            
        print("[PASS] Forensic Check: LBA filled with randomized pseudo-noise. No residue.")

if __name__ == "__main__":
    create_dummy()
    simulate_triple_wipe()
    forensic_check()
