# KAIRO-FW User Manual

After installing the `kairo-fw` package, you can use the `kairo` command to interact with the system's security surface.

## Running the tool

To use the tool, open your terminal and run:
```sh
kairo <command>
```

## Available Commands

### `kairo status`
**Purpose:** Shows global health and the current split model summary (core/net/sec).
**Expected Result:**
```
KAIRO Status: Healthy
Split Model: core/net/sec
```

### `kairo security matrix status`
**Purpose:** Displays the status of the security response matrix, including counts for CVE and non-CVE threat records.
**Expected Result:**
```
KAIRO Security Response Matrix Status
  CVE Records:     5
  Non-CVE Threats: 10
  Source (CVE):    /usr/share/kairo-fw/rules/kairo-cve-response.toml
  Source (Other):  /usr/share/kairo-fw/rules/kairo-non-cve-threat-response.toml
```

### `kairo forbidden-scan`
**Purpose:** Verifies the absence of retired or forbidden components on the system.
**Expected Result:**
`Forbidden scan: PASS (No retired components found)`

### `kairo split`
**Purpose:** Displays the status of the Core/Net/Sec separation.

### `kairo agents list`
**Purpose:** Lists all registered AI agents and their current state.

### `kairo version` (Built-in)
**Purpose:** Shows version information for the KAIRO Management CLI.

## Note for Users
The `kairo` tool is the real KAIRO Management interface. It provides visibility into the security rules, AI agent lifecycle, and system posture. While it provides comprehensive verification, it does not perform automatic network filtering or firewall changes in this release.
