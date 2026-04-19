# tuffutl Command Reference (for Users & Admins) v1.1.0

**Last Updated**: March 22, 2026  
**Target**: TUFF-OS Administrators and Users operating from the Upper OS

`tuffutl` is the sole authorized management interface for TUFF-OS. It mediates all physical layer operations and completely isolates direct access from the Upper OS.

---

## 1. Execution Modes

| Mode | Command Example | Use Case | Notes |
|:---|:---|:---|:---|
| **CUI Mode** | `tuffutl --cui` | Troubleshooting / Offline | Persistent in VGA text mode. |
| **Backend / IPC** | Service-based | Normal Ops (via Web-UI/Scripts) | Operates on ZRAM. |
| **One-shot** | `tuffutl sys status` | Scripting / Automation | Recommended. |

---

## 2. Common Options

| Option | Meaning | Default |
|:---|:---|:---|
| `--json` | Output in JSON format. | None |
| `--verbose` / `-v` | Enable detailed logging. | None |
| `--dry-run` | Simulate without execution. | None |
| `--help` | Display help information. | — |

---

## 3. Command List (By Category)

### 3.1 sys (System Management) — root Only

| Command | Args / Options | Description | Returns / Errors |
|:---|:---|:---|:---|
| `sys status` | `--detail` | Display holistic system state (Genesis, 3N, Isolation, Disk Pool, KAIRO). | JSON / Text |
| `sys cpuinfo` | — | CPU Microarch and SIMD/AVX support status. | Text |
| `sys reboot` | — | Secure Reboot (Logout + Isolation flag preservation). | OK / ERR_BUSY |
| `sys poweroff` | — | Secure Shutdown. | OK |
| `sys isolation status` | — | Current Isolation Mode status. | Active / Inactive |
| `sys isolation trigger` | — | Manual transition to Isolation (for testing). | OK |
| `sys isolation recover` | `--pin <PIN>` | Release Isolation (PIN required). | OK / ERR_PIN |

### 3.2 user (User Management) — root / admin

| Command | Args / Options | Description | Privileges |
|:---|:---|:---|:---|
| `user add <login_id>` | `--password <pw>` | Create new user (Mandatory password change flag). | root |
| `user del <login_id>` | — | Delete user (Immediate session destruction). | root |
| `user list` | `--all` | List all users (Status, TagGroupMask). | root / admin |
| `user reset <login_id>` | `--force` | Reset password (Zero-init + Forced-change). | root / admin |
| `user password <login_id>`| `--new <pw>` | Change password (root can change others). | Active session |
| `user tag <login_id>` | `--add/--remove <tag>` | Edit TagGroupMask. | root / admin |

### 3.3 fs (File System Management) — root / Folder Owner

| Command | Args / Options | Description | Note |
|:---|:---|:---|:---|
| `fs status` | `--detail` | Overall TUFF-FS state (N-Redundancy, J-Generation, UQ, Emergency Area). | — |
| `fs commit` | `--target <path>` | Finalize changes to physical sectors (N-Replica swap). | — |
| `fs reject` | `--target <path>` | Discard changes (Rollback to last commit). | — |
| `fs rollback <epoch>`| `--target <path>` | Rollback path to a specific generation ID. | J-Generation ONLY |
| `fs fsck` | `--repair` | Integrity check & auto-repair. | — |
| `fs nozram` | — | Force ZRAM flush (for debugging). | — |
| `fs tag add/remove` | `<path> <tag>` | Assign/Remove security tags. | — |
| `fs tag list` | — | List tags for a specific path. | — |

**IMPORTANT**: `rollback` is restricted to **J-Generation paths** (generative folders). Executing against N-Redundancy areas returns `Attribute Error: Not a Generational Path`.

### 3.4 nw (Network Management) — root / Network Admin

| Command | Args / Options | Description |
|:---|:---|:---|
| `nw status` | `--live` | KAIRO state (Blacklist, AI-Server List, eBPF rules). |
| `nw blacklist add/del` | `<ip/cidr>` | Add or remove entries from the blocklist. |
| `nw blacklist refresh` | — | Synchronize with external lists. |
| `nw aiserverlist add` | `<url> --password <pw>`| Add authorized AI server URLs. |
| `nw policy edit` | — | Interactive eBPF Rescue Allow-list editor. |

### 3.5 Miscellaneous

| Command | Args / Options | Description |
|:---|:---|:---|
| `version` | — | Version information. |
| `help` | `[command]` | Help (Full list if no args). |
| `log tail` | `--lines <n>` | Real-time `witness.log` monitor. |
| `test harness` | `--forge-token <n>` | Testing only (Token forgery, stress generation). |

---

## 4. Return Codes & Errors

| Code | Meaning | Typical Scenario |
|:---|:---|:---|
| 0 | Success | Operation completed. |
| 1 | Generic Error | Internal system error. |
| 2 | Forbidden | Standard user executing root commands. |
| 3 | Attribute Error | Calling `rollback` on non-J path. |
| 4 | Secret / Not Found | Access denied to unauthorized tag (intentional hiding). |
| 5 | Isolation Active | Command rejected while system is in Isolation Mode. |
| 6 | Consensus Failure | 3N majority vote mismatch detected. |
| 7 | Pin Required | Isolation recovery PIN required. |
| 8 | Resource Exhausted| UQ 80% back-pressure or disk full. |

---

## 5. Common Examples

```bash
# Check system status (Most frequent)
tuffutl sys status --detail

# Create a new user
tuffutl user add alice --password "TempPass123!"

# Tag a confidential folder
tuffutl fs tag add /data/secret "Confidential"

# Add a network block
tuffutl nw blacklist add 192.168.1.100 --reason "malicious"

# Rollback (Ransomware countermeasure)
tuffutl fs rollback 42 --target /data/project
```

**CAUTION**: Since all operations are **directly linked to the physical layer**, there is a risk of data loss due to incorrect commands. **Always maintain backups and practice in a test environment** before production use.
