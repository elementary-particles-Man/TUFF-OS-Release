# TUFF-OS Installation Guide (User Edition)

**Last Updated**: March 22, 2026
**Target**: Users wishing to deploy TUFF-OS on Windows 11 / Linux hosts.
**Estimated Duration**: Approx. 60–120 minutes (depending on storage initialization time).

### CRITICAL WARNING (Read First)

- **RISK OF TOTAL DATA LOSS**:
  The TUFF-OS installer will **physically erase all data on target disks**.
  → Please **perform a full backup of all important data to a separate drive** beforehand.
- **Multiple HDDs**: You can use HDDs from different manufacturers and capacities. However, using **at least 3 drives** (5 recommended) significantly improves the reliability of 3N redundancy.
- **DO NOT POWER OFF**: Do not cut power during installation. While the system is designed with automatic rollback, using a **UPS** is highly recommended.

### Prerequisites

1. **Host Machine** (Windows 11 or Linux)
2. **USB Flash Drive** (8GB+, FAT32 formatted)
3. **Physical HDDs** (Minimum 3 recommended, SATA connection)
4. **Internet Connection** (Required only for initial build)
5. **Administrative Privileges** (sudo / Run as Administrator)

---

### Procedure

#### Step 1: Obtain the Installer
1. Download the latest installer from the official repository.
   - Example filename: `tuff-installer-latest.exe` (Windows) / `tuff-installer` (Linux)
2. Verify the **SHA256 hash** to ensure the installer has not been tampered with (Recommended).

#### Step 2: Backup and Disk Preparation
1. Copy all important data to a **completely separate drive**.
2. Connect the target HDDs (Minimum 3, recommended 5).
3. Identify the device names using **Disk Management** (Windows) or `lsblk` (Linux).
   - Examples: Windows → `\\.\PhysicalDrive2`, Linux → `/dev/sdb`.
   - **Specifying the wrong drive will result in data loss.** Double-check carefully.

#### Step 3: Launch the Installer
**Windows**:
1. Run `tuff-installer-latest.exe` with Administrator privileges.
2. Click "Agree" to proceed.

**Linux**:
```bash
sudo chmod +x tuff-installer
sudo ./tuff-installer
```

#### Step 4: Installation Wizard
1. **Language Selection**: Select your preferred language.
2. **Target Disk Selection**:
   - System Destination (SSD recommended): Select 1 drive.
   - Data JBOD Pool (HDD): Select 3 or more drives (Ctrl+Click for multiple).
3. **Warning Dialog**:
   - "Data on all selected drives will be permanently erased. Proceed?"
   - Click **"YES, I UNDERSTAND"** twice (Double confirmation).
4. **Initial Configuration**:
   - Administrator Password (12+ characters recommended).
   - Save location for **TUFFkey.json** (USB drive recommended).
5. **Start Installation**:
   - Duration: Approx. 20–60 minutes (depending on drive count and capacity).

#### Step 5: Post-Installation Verification
1. Upon reboot, select "TUFF-OS" from the **UEFI Boot Menu**.
2. Genesis initialization will run automatically on first boot (Takes a few minutes).
3. Success if the **tuffutl** icon appears on the desktop.
4. Run the following in Command Prompt / Terminal to verify state:

```bash
tuffutl sys status
```

Expected Output:
```
Genesis: Valid (3N majority OK)
UserAuthDB: Initialized (3 replicas detected)
Isolation: Inactive
Disk Pool: Healthy (5/5 drives)
```

#### Step 6: Create First User (Mandatory)
```bash
tuffutl sys user add yourname --password "StrongPass123!"
```
- A **password change is forced** upon first login.
- After changing, start your session with `tuffutl sys login`.

---

### Troubleshooting (Common Issues)

| Symptom | Possible Cause | Action |
|:---|:---|:---|
| Installer stops mid-way | Power cut / Disk failure | Check logs, then restart installation (auto-rollback). |
| `tuffutl sys status` says Genesis Invalid | 2+ disks in 3N set damaged | Replace damaged disks → `tuffutl fs fsck --repair`. |
| HDDs invisible from Upper OS | Normal (TUFF-OS hides them) | HDDs are mounted only after `tuffutl sys login`. |
| High CPU during 10Gbps load | Vulkan Passthrough missing | Verify `-device vfio-pci` in QEMU/KVM config. |

---

### Recommended Post-Install Settings

1. Register dangerous IPs: `tuffutl nw blacklist add`.
2. Create a backup admin: `tuffutl sys user add admin2 --admin`.
3. Regularly run `tuffutl fs commit` (Manual or scheduled).
4. Tag important folders (e.g., "Confidential").

**Safety First**: TUFF-OS protects data at the physical layer, but **backups before installation are the user's responsibility.** Understand the risk of data loss before use.
