# tuffutl Command Reference & Backend Specifications (v1.1.0)

**Last Updated**: March 22, 2026  
**Target**: TUFF-OS Administrators, Developers, and Advanced Users

`tuffutl` is the sole authorized management interface for TUFF-OS. It mediates all physical layer operations and completely isolates direct access from the Upper OS.

---

## 1. Execution Modes & Basic Rules

| Mode | Command | Primary Use Case | Notes |
|:---|:---|:---|:---|
| **CUI Mode** | `tuffutl --cui` | Offline / Troubleshooting | Persistent in VGA text mode. |
| **Backend / IPC** | Service-based | Web-UI / Scripting | Operates on ZRAM. |
| **One-shot** | `tuffutl <cmd> [args]` | Automation / Batch jobs | `--json` recommended for machine reading. |

**Common Options** (Available for all commands)

| Option | Meaning | Example |
|:---|:---|:---|
| `--json` | Output in JSON format. | `tuffutl sys status --json` |
| `--verbose` / `-v` | Enable detailed logging. | Use `-vv` for max verbosity. |
| `--dry-run` | Simulate without execution. | For safety verification. |
| `--help` | Display help information. | `tuffutl fs commit --help` |

---

## 2. Privilege Levels

- **root**: Full physical layer access (Privileged user created at installation).
- **admin**: Administrative privileges granted by root.
- **user**: Standard user (Access controlled via TagGroupMask).

---

## 3. Command List (By Category)

### 3.1 sys (System Management) — root Only

| Command | Args / Options | Description | Returns / Errors |
|:---|:---|:---|:---|
| `sys status` | `--detail`, `--json` | Display holistic system state (Genesis, 3N, Isolation, Disk Pool, KAIRO). | JSON / Text |
| `sys cpuinfo` | `--json` | CPU Microarch, SIMD/AVX-512/VAES support status. | Text / JSON |
| `sys reboot` | — | Secure Reboot (Session Logout + Isolation flag preservation). | 0 / ERR_BUSY |
| `sys poweroff` | — | Secure Shutdown. | 0 |
| `sys isolation status` | `--json` | Current Isolation Mode status. | Active / Inactive |
| `sys isolation trigger` | — | Manual transition to Isolation (for testing). | 0 |
| `sys isolation recover` | `--pin <PIN>` | Release Isolation (PIN bound to Genesis required). | 0 / ERR_PIN |

### 3.2 user (User Management) — root / admin

| Command | Args / Options | Description | Privileges |
|:---|:---|:---|:---|
| `user add <login_id>` | `--password <pw>` | Create new user (Forced-change flag ON). | root |
| `user del <login_id>` | — | Delete user (Immediate session destruction). | root |
| `user list` | `--all`, `--json` | List all users (Status, TagGroupMask). | root / admin |
| `user reset <login_id>` | `--force` | Reset password (Zero-init + Forced-change ON). | root / admin |
| `user password <login_id>`| `--new <pw>` | Change password (root can change others). | Logged-in only |
| `user tag <login_id>` | `--add/--remove <tag>` | Edit TagGroupMask. | root / admin |

### 3.3 fs (File System Management) — root / Folder Owner

| Command | Args / Options | Description | Note |
|:---|:---|:---|:---|
| `fs status` | `--detail`, `--json` | Overall TUFF-FS state (N-Redundancy, J-Generation, UQ, Emergency Area). | — |
| `fs commit` | `--target <path>` | Finalize changes to physical sectors (N-Replica pointer swap). | Attribute Error if non-N path. |
| `fs reject` | `--target <path>` | Discard changes (Rollback to last commit). | Attribute Error if non-N path. |
| `fs rollback <epoch>`| `--target <path>` | Rollback path to a specific generation ID. | **J-Generation paths only.** |
| `fs fsck` | `--repair`, `--json` | Integrity check & auto-repair. | Consensus Failure if 3N broken. |
| `fs nozram` | — | Force ZRAM flush (for debugging). | — |
| `fs tag add/remove` | `<path> <tag>` | Assign/Remove security tags. | Permission Denied if unauthorized. |
| `fs tag list` | `--json` | List tags for a specific path. | — |

**CRITICAL RULE**: `rollback` is restricted to **J-Generation paths** (generative folders). Executing against N-Redundancy areas returns `Attribute Error: Not a Generational Path`.

### 3.4 nw (Network Management) — root / Network Admin

| Command | Args / Options | Description |
|:---|:---|:---|
| `nw status` | `--live`, `--json` | KAIRO state (Blacklist, AI-Server List, eBPF rules). |
| `nw blacklist add/del` | `<ip/cidr>` | Add or remove entries from the blocklist. |
| `nw blacklist refresh` | — | Synchronize with external lists. |
| `nw aiserverlist add` | `<url> --password <pw>`| Add authorized AI server URLs. |
| `nw policy edit` | — | Interactive eBPF Rescue Allow-list editor. |

### 3.5 Utilities

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

## 5. Implementation Rules (for Admins & Devs)

1. **Attribute Separation**: `rollback` MUST be restricted to J-Generation paths.
2. **Metadata cache (FMC)**: `tuffutl` determines path attributes immediately from the File Metadata Cache.
3. **Failure Behavior**: Physical layer errors (Consensus Failure) should trigger a proposal to transition to **Isolation Mode**.
4. **JSON Structure**:
   ```json
   {
     "status": "success" | "error",
     "code": 0 | 1 | 2 | ...,
     "message": "Detail message",
     "data": { ... }
   }
   ```
