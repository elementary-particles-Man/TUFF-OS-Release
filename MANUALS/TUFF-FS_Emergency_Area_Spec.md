# TUFF-FS Emergency Area Specification

The Emergency Area is a core fault-tolerance feature of TUFF-FS, which "**consistently reserves 10% of each HDD to automatically evacuate data to other healthy disks in the event of a failure.**"

### 1. Overview of the Emergency Area (Diagram)

```mermaid
flowchart TD
    subgraph "Physical HDDs (e.g., 3 x 2TB)"
        direction TB
        
        subgraph HDD1
            D1[Data Area<br>1.8TB] --> E1[Emergency Area<br>200GB / 10%]
        end

        subgraph HDD2
            D2[Data Area<br>1.8TB] --> E2[Emergency Area<br>200GB / 10%]
        end

        subgraph HDD3
            D3[Data Area<br>1.8TB] --> E3[Emergency Area<br>200GB / 10%]
        end
    end

    subgraph "Failure Event (HDD2 Failure)"
        F[Failure Detected<br>SMART Anomaly / No Response] -->|Auto-Evacuation Starts| E1
        E1 -->|Data Transfer| E3
        E3 --> R[3N Recovery Complete<br>Continuous Operation]
    end

    subgraph "Adding New HDD"
        N[Insert New HDD<br>/dev/sdg] --> S[Transfer Emergency Area Data]
        S --> R
    end

    classDef hdd fill:#1e40af,stroke:#60a5fa,color:#fff
    classDef emergency fill:#166534,stroke:#4ade80,color:#fff
    classDef data fill:#854d0e,stroke:#fbbf24,color:#fff
    classDef failure fill:#991b1b,stroke:#fca5a5,color:#fff
    classDef recovery fill:#065f46,stroke:#6ee7b7,color:#fff

    class HDD1,HDD2,HDD3 hdd
    class E1,E2,E3 emergency
    class D1,D2,D3 data
    class F failure
    class R,S recovery
```

### 2. Detailed Operational Flow (Step-by-Step)

```mermaid
sequenceDiagram
    participant User as Upper OS
    participant FS as TUFF-FS
    participant Monitor as Monitoring Thread
    participant Disk as Physical HDD Group

    User->>FS: Mass Write Request
    FS->>Disk: Distributed Write to Normal Area
    Monitor->>Disk: SMART/Response Monitoring (Continuous)
    alt Failure Detected (SMART Anomaly / Timeout)
        Monitor->>FS: HDD2 Failure Notification
        FS->>Disk: Start Evacuating HDD2 Data to Emergency Area
        Disk->>Disk: Utilize Emergency Areas of HDD1 / HDD3 / HDD4
        FS->>User: Background Evacuation (No Impact on Upper OS)
        Disk->>FS: Evacuation Complete Report
        FS->>User: 3N Recovery Notification
    else New HDD Added
        User->>FS: New HDD /dev/sdg Connected Notification
        FS->>Disk: Emergency Area Data → Sync to New HDD
        Disk->>FS: Sync Complete
        FS->>User: 3N Full Recovery Complete
    end
```

### 3. Key Rules and Specifications

| Item | Specification Detail | Remarks / Benefits |
|:---|:---|:---|
| **Reservation Rate** | **10%** of total HDD capacity (default, configurable) | Always ensures minimum evacuation capacity. |
| **Placement** | End of each HDD (allocated backwards from last LBA) | Optimized for sequential writes. |
| **Usage Timing** | Failure/Anomaly detected in 1 HDD → Uses others' Emergency Area | Maintains 3N without downtime. |
| **Re-sync (Rebuild)** | Auto-transfers data from Emergency Area upon new HDD insertion | Supports hot-swapping. |
| **In Isolation Mode** | New evacuations to Emergency Area are halted (All reservations locked) | Complete freeze during final defense. |
| **Capacity Depletion** | Emergency Area full → Temporarily suspends new writes | Works with UQ back-pressure to prevent data loss. |
| **Monitoring Interval** | SMART check: Every 1 min<br>Response timeout: Detected after 5s x 3 failures | Early discovery and early evacuation. |

### 4. Operational Tips for Administrators

- **Periodic Check Command**
  ```bash
  tuffutl fs status --detail | grep Emergency
  ```
  → Displays the usage rate and free capacity of each HDD's Emergency Area.

- **Forced Evacuation Test (Drill)**
  ```bash
  tuffutl fs emergency simulate --disk /dev/sdc
  ```
  → Treats one HDD as a simulated failure → Verify evacuation behavior (Recommended for test environments).

- **Recommendation for New HDDs**
  Ensure the capacity is equal to or greater than existing HDDs to prevent Emergency Area shortage.

- **Alert on Depletion**
  Warnings are logged to witness.log if usage exceeds 90% (Optional notification to Upper OS).

---

### Summary

The TUFF-FS Emergency Area is the mechanism at the heart of TUFF-OS fault tolerance: "**Even if one physical disk dies suddenly, the system continues to maintain 3N redundancy without downtime by utilizing the free space on other HDDs.**"

- 10% always reserved → Acts as an evacuation buffer.
- Anomaly detection → Automatic evacuation starts.
- New HDD added → Automatic re-sync for full recovery.

This ensures that **a physical disk failure does not lead to a system-wide shutdown**, realizing the absolute resilience of TUFF-OS.
