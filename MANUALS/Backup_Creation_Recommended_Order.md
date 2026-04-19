# Backup Creation: Recommended Procedures

The following outlines the detailed procedures for **creating backups** in **TUFF-OS**.
Use these steps for:
- Pre-reinstallation prevention (e.g., if you lost your PIN).
- Insurance before physical disk replacement.
- Data preservation during periodic maintenance.
- Migration from test to production environments.

### CRITICAL WARNING (Read First)

- **DO NOT POWER OFF** during the backup process. While TUFF-OS is designed with auto-rollback, a **UPS** is highly recommended.
- Store backups on **secure storage outside of TUFF-OS** (e.g., external HDD, NAS, or encrypted cloud).
- **Encrypt** your backup files (using GPG or VeraCrypt).
- **Always verify integrity** immediately after the backup is created.

---

### Backup Procedures (Recommended Order)

#### Method A: Logical Backup (Most Recommended / Fast / Flexible)

**Target**: Critical folders (e.g., `/data`, `/home`).  
**Pros**: Small file size, easy to restore.  
**Duration**: Minutes (depending on data volume).

1. **Log In to Active Session**
   ```bash
   tuffutl sys login <your_id> --password <your_pw>
   ```

2. **Execute Backup Command** (with recommended options)
   ```bash
   tuffutl backup create \
     --target /data \                  # Root path to backup (can specify multiple)
     --output /mnt/external/backup_$(date +%Y%m%d).tar.zst \
     --compress zstd \                 # High compression (zstd recommended)
     --encrypt --key /path/to/keyfile \ # Optional but highly recommended
     --verify \                        # Auto-verify after creation
     --exclude "/data/temp"            # Optional exclusion
   ```

3. **Verify Completion**
   ```bash
   tuffutl backup verify --file /mnt/external/backup_20260322.tar.zst
   ```
   - Success if "Verification successful" is displayed.

4. **Move Backup to Secure Storage**
   - Copy to external HDD / NAS / Encrypted USB.
   - Follow the **3-2-1 Rule**: 3 copies, 2 different media, 1 offsite.

#### Method B: Full Physical JBOD Backup (For Complete Reconstruction)

**Target**: The entire contents of all HDDs.  
**Pros**: Enables complete state restoration.  
**Cons**: Extremely large file size (Multiple TBs).

1. **Shutdown the System** (Recommended)
   ```bash
   tuffutl sys poweroff
   ```

2. **Image each HDD via `dd`** (Execute on the Linux host)
   ```bash
   # Repeat for each HDD (e.g., sdb, sdc, sdd)
   dd if=/dev/sdb of=/mnt/backup/jbod_sdb_$(date +%Y%m%d).img bs=4M status=progress conv=fsync
   dd if=/dev/sdc of=/mnt/backup/jbod_sdc_$(date +%Y%m%d).img bs=4M status=progress conv=fsync
   # ... repeat as necessary
   ```

3. **Verify Integrity**
   ```bash
   sha256sum /mnt/backup/jbod_*.img > checksums.txt
   ```

4. **Secure the Images**
   - Copy to multiple external locations.
   - **Encryption is mandatory** (VeraCrypt container recommended).

#### Method C: Incremental Backup (For Daily Operations)

**Target**: Changes since the last full backup.  
**Pros**: Saves space and time.

```bash
tuffutl backup incremental \
  --base /mnt/backup/full_20260301.tar.zst \
  --target /data \
  --output /mnt/backup/incremental_20260322.tar.zst \
  --verify
```

---

### Mandatory Checklist Post-Backup

1. **Integrity Verification**: Always run `tuffutl backup verify`.
2. **Restore Test**: Periodically perform a restore on a test VM to ensure TagGroupMasks and permissions are correctly recovered.
3. **PIN Linkage**: Re-verify your Recovery PIN at the same time as the backup. **A backup is useless without a PIN if you need to reinstall.**

---

### Backup Best Practices

- **Strictly follow the 3-2-1 Rule.**
- **Schedule regular backups**: Weekly full, daily incremental.
- **Mandatory Encryption**: Never store backups in plain text.
- **Test Restoration**: Conduct a drill every 3 months.
- **PIN Management**: Store the PIN on a **separate medium** (e.g., paper in a safe).

**Summary**
Backup is the "**Final Insurance**" for TUFF-OS. Even in the face of PIN loss or total physical destruction, a backup ensures recovery with minimal downtime. **Losing the backup itself results in total loss.** Make backup management your highest operational priority.
