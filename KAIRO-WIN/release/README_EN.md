# KAIRO-WIN: Absolute AI-Proxy Shield (Windows Edition)

KAIRO-WIN is a high-performance, deterministic AI-proxy firewall designed to provide humans with absolute mastery over AI agent communications. By leveraging the Windows Filtering Platform (WFP) and Vulkan-accelerated signature verification, KAIRO-WIN ensures that not a single unauthorized bit enters or leaves your system.

## Technical Core: The Invariant Shield
*   **WFP Shim (Kernel-Level):** Intercepts network traffic at the lowest possible layer, bypassing the non-deterministic nature of the WinSock API.
*   **Vulkan Acceleration:** Offloads signature verification to the GPU, achieving sub-1ms latency even under extreme "Gai-level" packet loads.
*   **Binary Integrity:** All communications are bound to the `AITcpPacket` structure. Any deviation results in immediate, silent neutralization.

## Features
*   **Zero Feedback Policy:** Unauthorized packets are dropped silently. No TCP resets, no ICMP errors—just pure, clinical erasure.
*   **Constant-Time Verification:** Immune to timing side-channel attacks. The Reaper's judgment remains constant.
*   **Human Sovereignty:** The true essence of KAIRO-WIN lies in humans completely mastering the communication of AI agents.

## Installation
Run `KAIRO-SHIELD-INSTALLER.exe` as Administrator. The service will be registered to `C:\Program Files\THP-lab\KAIRO-FW` and set to start automatically.

## License
MIT License. Free to use, free to master.
