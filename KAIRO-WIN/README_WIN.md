# KAIRO-WIN (Windows Edition)

「先人への恩返し」を具体的な「防壁（コード）」として表現する、Windows版 KAIRO Firewall。

## 思想 (RADICAL Adaptation)

Linux版 KAIRO-FW の「純粋な論理」を抽出し、Windowsという非決定論的なカオスに「楔（くさび）」を打ち込みます。

### 1. 外科切除 (Excised)
* **Systemd:** 全て排除。`cargo-deb` 依存も切除。
* **Unix Sockets:** Windows互換の Named Pipes または Local Loopback へ抽象化。
* **Linux Paths:** `/etc/kairo-fw` 等のPOSIXパスを排除し、環境変数ベースへ移行。

### 2. 適応 (Added)
* **WFP Shim:** Windows Filtering Platform による低レイヤーパケット拘束。
* **Windows Service:** システム起動時からの「1（稼働）」状態維持。
* **WinSock Bypass Guard:** 通信の揺らぎを抑え、`AITcpPacket` の構造を強制。

### 3. 不変 (Invariant)
* **rust-core:** 決定論的ロジック、FlatBuffersバイナリ化、署名検証。
* **Vulkan Engine:** GPUによる冷徹な演算加速（clear-mini）。

## ディレクトリ構造

* `rust-core/`: 論理層の核。
* `clear-mini/`: Vulkan演算エンジン。
* `src/kairo-win-service/`: Windowsサービス・WFPアダプタ。
* `src/kairo-win-lib/`: 通信・設定管理ライブラリ。
* `src/kairo-win-f/`: コマンドライン・インターフェース。

## ビルド・インストール

Windows環境にて `cargo build --release` を実行後、`install_win.ps1` を管理者権限で実行してください。
