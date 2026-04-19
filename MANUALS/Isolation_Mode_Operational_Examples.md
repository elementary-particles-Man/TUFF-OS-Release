# Isolation Mode Operational Examples (Real-World Scenarios)

#### Scenario 1: External Impersonation Attack (Most Frequent Case)

**Situation**
- An attacker attempts to log in multiple times using a stolen session token (e.g., automated brute-force via script).
- Token verification fails three consecutive times for the same User ID.

**Flow to Isolation Trigger**
1. 1st & 2nd Attempts: Token mismatch → Event logged (witness.log: "Token validation failed x1/x2").
2. 3rd Attempt: Immediate transition to Isolation Mode.
   - Session keys and TagGroupMask on ZRAM are zeroized in bulk using AVX2/AVX-512.
   - All physical I/O is blocked (Read → Infinite Noise, Write → Fake Success).
   - Network defense rules switch to "Drop All Packets".
   - "Isolation Persistent" flag is set in TuffHandoffBlock V1.

**Operational Response**
- Admin Notification: witness.log + optional Email/Slack alerts.
- Recovery: `tuffutl sys isolation recover --pin <12-digit PIN>`.
- Prevention: Forced password change via `tuffutl user reset <login_id> --force`.

**Duration**: Immediate lockout after the third failed verification.

#### Scenario 2: Large-Scale DDoS Attack (AI Agent Driven)

**Situation**
- Thousands of AI agents simultaneously send SYN Flood + L7 malicious payloads (e.g., "exfiltrate secrets" prompts).
- Sustained traffic at 10Gbps levels.

**Flow to Isolation Trigger**
1. The network defense layer performs initial Silent Drop of most packets (CPU usage near 0%).
2. Vulkan GPGPU AI Probe detects anomalous patterns (4096 packets classified simultaneously).
3. Attack persists for 5 minutes → Automatic Isolation trigger upon threshold exceeding.
   - All sessions zeroized.
   - Complete network blackout (full-drop rules applied).
   - Physical I/O blocked simultaneously.

**Operational Response**
- Auto-Notification: "High volume DDoS detected → Isolation" recorded in witness.log.
- Recovery: After PIN entry, re-adjust policies via `tuffutl nw policy edit`.
- Forensic Audit: Verify all-discard logs with ML-DSA signatures via `tuffutl log tail`.

**Duration**: Transition begins immediately after threshold detection and completes as the isolation workflow finishes.

#### Scenario 3: Physical Disk Tampering Detection (Evil Maid Scenario)

**Situation**
- An attacker gains physical access, removes one HDD, tampers with it, and returns it.
- Inconsistency occurs in 3N majority vote.

**Flow to Isolation Trigger**
1. Genesis verification at boot → 3N Mismatch (Consensus Failure).
2. Automatic transition to Isolation Mode.
   - All sessions discarded.
   - Physical I/O blocked (disks become invisible to the Upper OS).
   - "Consensus Failure → Isolation" recorded in witness.log.

**Operational Response**
- Admin Action: Replace damaged HDD → `tuffutl fs fsck --repair`.
- Recovery: Release Isolation via PIN entry → Re-synchronization starts.
- Prevention: Restrict physical access (locked server room, camera surveillance).

**Duration**: Immediate upon boot.

#### Scenario 4: Manual Trigger (Testing & Training)

**Situation**
- Intentionally triggering Isolation for security drills or system tests.

**Operating Procedure**
```bash
# Execute with Administrator/Root privileges
tuffutl sys isolation trigger
```

**Results**
- Immediate total I/O block + Zeroize.
- "Manual Isolation Triggered" recorded in witness.log.
- Release: `tuffutl sys isolation recover --pin <PIN>`.

**Duration**: Command-initiated lockout completes immediately.

### Isolation Mode Operational Principles (Summary for Admins)

1. **Immediate & Automatic Trigger**
   - Does not wait for human intervention; blocks within 0.1s of anomaly detection.

2. **Strict Recovery Requirements**
   - Recovery PIN is mandatory. If PIN is lost, re-installation (total data loss) is required.

3. **Persistent Across Reboots**
   - Flags are passed via HandoffBlock; rebooting by an attacker will not bypass the lockout.

4. **Complete Forensic Evidence**
   - Trigger cause, time, and impact scope are recorded with PQC signatures (tamper-proof).

5. **User Experience**
   - Normal operation: Zero impact.
   - Isolated state: "System Isolated" screen + All access denied.

---

**Summary**
Isolation Mode is the final defense mechanism embodying the TUFF-OS philosophy: "**Isolate immediately upon suspicion.**"  
Thanks to physical layer absolute integrity and asynchronous zero-copy design, the system **continues defense permanently with minimal load** after activation.
