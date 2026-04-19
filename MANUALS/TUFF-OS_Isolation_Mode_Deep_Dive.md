# TUFF-OS Isolation Mode: Detailed Explanation

Isolation Mode is the **final line of defense** for TUFF-OS.
It is designed to disconnect the system as soon as unauthorized access or an anomaly is detected. It activates when other security mechanisms (Deception, 3N Redundancy, the network defense layer, etc.) are bypassed or as the final layer of a multi-depth defense strategy.

### 1. Trigger Conditions (The Instant of Activation)

Isolation Mode activates immediately upon any of the following (ordered by priority):

1. **Forged Token / Impersonation Detection** (Most frequent trigger)
   - Failure to verify an HMAC-SHA3-256 signed session token for three consecutive attempts.
   - Duplicate login attempts for the same user (Session ID collision) x3 times.

2. **Network Defense Layer Anomaly Detection**
   - Sustained 10Gbps SYN Flood + L7 payload attacks.
   - IDPI (AI Probe) detects "malicious response patterns" (e.g., prompts targeting secret exfiltration).

3. **Genesis / 3N Consensus Failure**
   - 3N majority vote mismatch on two or more disks (Suspected physical tampering).

4. **Manual Trigger** (For Administrators)
   - `tuffutl sys isolation trigger`

5. **Timeout / Abnormal Termination**
   - GPU offload response timeout exceeding 5ms.
   - Power failure detected during CommitPending state (unprocessed MQ).

### 2. Behavior upon Activation (What Happens)

When entering Isolation Mode, the following processes are executed **immediately and in order**:

1. **Immediate Destruction of All Sessions**
   - Bulk Zeroization of session keys and TagGroupMask on ZRAM using AVX2/AVX-512.
   - Forced Logout of all active sessions.

2. **Complete Physical I/O Blockade**
   - Reads from Upper OS → Return permanent random noise (ChaCha20 + LBA phase).
   - Writes → Mimic STATUS_SUCCESS while actually performing a Silent Drop.
   - File Creation / Directory Ops → Mimic ENOENT (Not Found).

3. **Total Network Blackout**
   - Network defense rules switch to "Drop All Packets."
   - `aiserverlist` and other allow-lists are temporarily disabled.

4. **Persistent Handoff Flag**
   - "Isolation Persistent" flag is set in TuffHandoffBlock V1.
   - Automatically restores the Isolated state upon the next reboot.

5. **Final Recording to witness.log**
   - Records the trigger cause, timestamp, and an ML-DSA quantum-resistant signature.
   - The log itself is immediately encrypted and added to the hash chain.

### 3. Deactivating Isolation (The Sole Recovery Path)

Deactivation is **strictly restricted**.

1. **Admin-Only Recovery PIN**
   - A PIN generated during installation and stored encrypted on a USB drive.
   - Command: `tuffutl sys isolation recover --pin <12-digit PIN>`

2. **Physical Reinstallation** (Last Resort)
   - Re-execute from the USB installer → Genesis re-initialization.
   - All old data is discarded (Fail-Closed).

**Standard users** cannot release the isolation.
**If the PIN is lost**, reinstallation (total data wipe) is the only method.

### 4. Behavior after Recovery

- Sessions are rebuilt from scratch (Re-login required).
- Network returns to KAIRO initial policies (Blacklist is preserved).
- "Isolation recovered" record is added to witness.log (PQC signed).

### 5. Practical Operational Examples (By Scenario)

| Scenario | Trigger | Result | Duration |
|:---|:---|:---|:---|
| Forged Token Attack | 3 Consecutive Failures | Total I/O Block + Zeroize | Immediate |
| Large-Scale DDoS | KAIRO GPU Detection | Continued Silent Drop + 0% CPU | Ongoing |
| Disk Tampering | 3N Mismatch | Boot Halt + Isolation | Immediate @ Boot |
| Ransomware Incursion | J-Generation Detection | Immediate Rollback + Isolation | Few ms |

---

### Summary: The Philosophy of Isolation Mode

- "**Isolate immediately upon suspicion.**"
- "**Leave no trace, hide existence.**"
- "**Continue defense across reboots.**"

Through these principles, TUFF-OS maintains **physical and permanent sovereignty** even against AI agents and state-level actors.
