# TUFF-OS Troubleshooting Guide

**Final Edition (for User Guide)**  
**Last Updated**: March 22, 2026  
**Target**: Administrators and Advanced Users  
**Principle**: **Always perform a full backup** before any operation. Due to the direct linkage to the physical layer, incorrect operations may lead to permanent data loss.

---

### 1. Startup & Boot Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| "TUFF-OS" not in UEFI menu | TUFFboot.efi missing / Low priority | 1. Change priority in BIOS/UEFI.<br>2. Reinstall from USB installer. | Check boot order after installation. |
| "Genesis Invalid" or "Consensus Failure" | 2+ disks in 3N set damaged / Disks swapped | 1. Run `tuffutl sys fsck --repair`.<br>2. Replace damaged disk → `tuffutl fs append /dev/sdX`. | Keep at least 3 HDDs connected. |
| Boots to Isolation Mode; login fails | 3 consecutive forged tokens | Run `tuffutl sys isolation recover --pin <PIN>` (using the PIN from Genesis creation). | Save PIN encrypted on a USB drive. |
| "TuffHandoffBlock V1 checksum error" | Handoff from UEFI to Kernel failed | 1. Power off → Reboot.<br>2. If it persists, perform a repair install from the USB. | Designed with auto-rollback for power cuts. |

### 2. File System (TUFF-FS) Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| `fs commit` / `reject` fails | UQ usage > 80% (Back-pressure) | Check usage via `tuffutl fs status` → Delete unneeded files in Upper OS. | Run `tuffutl fs commit` regularly. |
| `fs rollback` says "Not a Generational Path" | J-Generation not enabled | Run `tuffutl fs set-j /path` to enable, then retry. | Call `set-j` beforehand for important folders. |
| Data invisible after disk dropout | Emergency Area depleted | 1. Connect new HDD → `tuffutl fs append /dev/sdX`.<br>2. Wait for auto-resync. | Maintain at least 10% free space on all HDDs. |
| `fs fsck` reports "Consensus Failure" | 1 disk in 3N set completely dead | Physically replace HDD → `tuffutl fs fsck --repair`. | 3N is resilient up to 2 dead disks. |

### 3. User & Authentication Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| Login fails (Forgot password) | Change-on-first-login forgotten | Run `tuffutl user reset <login_id> --force` as root. | Password change is mandatory at first login. |
| Folders invisible via TagGroupMask | No permission (Intentional hiding) | Admin: `tuffutl user tag <login_id> --add <tag>`. | Assign tags like "Confidential" beforehand. |
| Session disconnected immediately | Currently in Isolation Mode | Run `tuffutl sys isolation recover`. | Automatic transition after 3 forged tokens. |

### 4. Network & KAIRO Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| Cannot connect to AI server | `aiserverlist` is OFF | `tuffutl nw aiserverlist on <password>`. | Password setup required at first use. |
| External connection blocked | IP registered in `blacklist` | `tuffutl nw blacklist del <ip>`. | Periodically check `nw blacklist list`. |
| Attack bypasses defense despite 0% CPU | Vulkan GPGPU passthrough missing | Reset `-device vfio-pci` in QEMU/KVM. | iGPU must be enabled on bare metal. |
| Large logs in `nw witness` | Ongoing DDoS attack | Check `tuffutl nw status --live` → Run `nw blacklist add` if needed. | Audit trails are tamper-proof via PQC signatures. |

### 5. Performance & Resource Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| UQ usage constantly > 80% | Heavy simultaneous writes | `tuffutl sys set --uqsize 16MB` (Effective next boot). | Run background `fs commit` periodically. |
| Slow despite < 0.4% disk util | ZRAM compression overhead | Temporarily release via `tuffutl fs nozram`. | 16GB+ RAM recommended. |
| SMART warnings detected | Physical HDD end-of-life | `tuffutl fs remove <HDD>` → Add new HDD. | Periodically check `tuffutl sys status`. |

### 6. Forensics & Recovery Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| TagGroupMask remains in memory dump | Zeroize failed | Manually force via `tuffutl sys isolation trigger`. | Auto-zeroized upon Isolation entry. |
| Inconsistency after power cut | Uncommitted MQ state | Run `tuffutl fs fsck --repair`. | Habituate regular commits. |
| Complete data loss | Total 3N failure | Restore via `tuffutl SSD restore` from backup. | External backups are mandatory. |

### 7. Update Integrity Issues

| Symptom | Possible Cause | Action | Prevention / Note |
|:---|:---|:---|:---|
| Update stops with a signature error | `patch_sig` / `full_iso_sig` mismatch or tampering | Abort immediately. Do not regenerate the ISO or delta on the client. | The intended user-facing message is `シグネチャが改ざんされているため処理を中止します`. |

---

### Final Advice

1. **Scheduled Maintenance**
   - Daily: `tuffutl sys status`
   - Weekly: `tuffutl fs fsck` + `tuffutl fs commit`
   - Monthly: `tuffutl nw blacklist refresh`

2. **Emergency Contacts**
   - Physical Failure: Connect new HDD immediately.
   - Isolation Lockout: Release with `recovery-pin`.
   - Irrecoverable Failure: `tuffutl SSD restore` from USB backup.

3. **Log Audit**
   ```bash
   tuffutl log tail --lines 100
   ```
   All operations and attacks are recorded in `witness.log` with PQC signatures.

---

This guide is fully aligned with all previous test results, design documents, and implementation specs. It serves as the final chapter of the user guide.
