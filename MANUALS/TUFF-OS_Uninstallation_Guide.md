# TUFF-OS Uninstallation Guide

**Last Updated**: March 22, 2026  
**Version**: 1.0 (Final Consensus Edition)

**[CRITICAL WARNING]** This operation is **completely irreversible**.

Uninstalling TUFF-OS will render the following **physically unrecoverable**:
- **All files and data** stored within TUFF-FS.
- Genesis blocks, UserAuthDB, and Emergency Areas.
- All security audit trails (with minor exceptions).
- Any traces of virtual drives as seen from the Upper OS.

**IF YOU HAVE NOT BACKED UP YOUR DATA, IT WILL BE PERMANENTLY LOST.**  
**Ensure a complete backup is performed before proceeding.**

---

## Overall Uninstallation Flow (Diagram)

```mermaid
flowchart TD
    A[Launch Uninstaller] --> B[3-Stage Confirmation]
    B -->|OK| C[Enter Terminal Mode<br>Independent Execution in RAM]
    C --> D[Forced Isolation Trigger]
    D --> E[Zeroize All Sessions<br>Bulk Wipe via AVX2/AVX-512]
    E --> F[Physical Storage Zero-Fill<br>dd if=/dev/zero]
    F --> G[USB Physical Key Wipe<br>KEY-CSE Destruction]
    G --> H[Remove UEFI Boot Entries<br>Delete TUFF-PAL Drivers]
    H --> I[Save Final Evidence (if --keep-log)<br>PQC-Signed witness.log]
    I --> J[Automatic Reboot]
    J --> K[Total Deletion of TUFF-OS]

    classDef danger fill:#991b1b,stroke:#fca5a5,color:#fff
    classDef step fill:#1e40af,stroke:#60a5fa,color:#fff

    class A,B,C,D,E,F,G,H,I,J,K step
    class D,E,F,G danger
```

## Danger Levels (Estimates)

```mermaid
flowchart LR
    A[Normal Operation] --> B[Warning Level 1<br>Data Loss Risk]
    B --> C[Warning Level 2<br>Physical Wipe Starts]
    C --> D[Final Stage<br>Irreversible Completion]

    classDef safe fill:#166534,stroke:#4ade80,color:#fff
    classDef warn fill:#854d0e,stroke:#fbbf24,color:#000
    classDef danger fill:#991b1b,stroke:#fca5a5,color:#fff
    classDef final fill:#4c1d95,stroke:#c084fc,color:#fff

    class A safe
    class B warn
    class C danger
    class D final
```

---

## Operating Procedure (Detailed Steps)

### Prerequisites (Mandatory)

1. **Perform a Full Backup** (Target outside TUFF-OS):
   ```bash
   tuffutl backup create --target / --output /mnt/external/full_backup_$(date +%Y%m%d).tar.zst --encrypt --verify
   ```
   → Ensure the backup destination is an **external storage device** separate from TUFF-OS.

2. **Obtain the Uninstaller**:
   - Copy the `tuff-uninstaller` binary to a USB drive.
   - Set execution permissions: `chmod +x tuff-uninstaller`.

### Execution Steps

1. **Launch with Administrator Privileges**:
   ```bash
   sudo ./tuff-uninstaller
   ```

2. **Stage 1 Confirmation (Warning 1)**:
   ```text
   [Warning Level 1]
   TUFF-OS will be completely uninstalled.
   All data, settings, and audit trails will be permanently erased.
   Recovery is impossible. Proceed? (yes/NO)
   ```
   → Type "**yes**" (all lowercase).

3. **Stage 2 Confirmation (Final String Input)**:
   ```text
   [Warning Level 2]
   Are you absolutely sure?
   Type "TUFF-OS完全削除" to confirm.
   ```
   → Type the exact string (case-sensitive).

4. **Stage 3 Confirmation (Backup Double-Check)**:
   ```text
   [Final Check]
   Have you created a backup? (yes/no)
   ```
   → Type "**yes**" to continue (typing "no" will abort immediately).

5. **Terminal Mode Transition (Automatic)**:
   - Copies itself to RAM disk (`/tmp/tuff-uninstaller-terminal`).
   - Terminates original process → Restarts from RAM.
   - Continues execution independent of physical Isolation locks.

6. **Physical Wipe Phase (Automatic Execution)**:
   - Immediate Zeroization of all sessions (AVX2/AVX-512).
   - Forced activation of Isolation Mode.
   - Zero-fills all target HDD areas (`dd if=/dev/zero`).
   - Auto-detects and zero-fills the USB Key area (KEY-CSE).
   - Removes UEFI entries (`efibootmgr`).
   - Removes TUFF-PAL drivers (`sc delete` / `rmmod`).

7. **Save Final Evidence (Only if --keep-log is specified)**:
   - Signs the final portion of `witness.log` via ML-DSA.
   - Saves to external USB (`/mnt/usb/final_evidence.log`).
   - Zero-fills the internal log files.

8. **Completion**:
   - Displays: "TUFF-OS has been completely erased. Please restart."
   - Executes an automatic `reboot` (unstoppable).

---

### Options List

| Option | Meaning | Default |
|:---|:---|:---|
| `--keep-log` | Save final evidence externally with PQC signatures. | None |
| `--no-terminal` | Disable RAM Terminal Mode (Dangerous/Not recommended). | Disabled |
| `--log-dest <path>` | Destination for evidence (e.g., /mnt/usb/final.log). | Auto-detect |
| `--dry-run` | Perform a simulation only. | None |

### Handling Special Cases

| Case | Response Method | Note |
|:---|:---|:---|
| Non-bootable system | Boot "Uninstall Mode" from USB installer → Zero-fill all disks. | No PIN required. |
| Lost PIN | Follow the procedure above. | Confirms total data wipe. |
| Remaining drivers | Win: Remove TUFF-PAL via Device Manager<br>Lnx: `rmmod` & `rm -rf` | Reboot mandatory. |
| Retain logs | Use `--keep-log --log-dest /mnt/usb/final.log`. | PQC-signed preservation. |

---

**Closing Word**  
TUFF-OS is a system that allows itself to "**return to nothing, just as it appeared from nothing.**"

Uninstallation is the final chapter of that philosophy.

**Always backup before execution.** And press the button **with absolute resolve.**
