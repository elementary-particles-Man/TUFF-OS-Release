# KAIRO-WIN (Userland) 詳細リファレンス

KAIRO-WIN は、Windows 環境向けに最適化された AI プロキシ・シールドです。
NDIS (Network Driver Interface Specification) 境界を通じて、AI エージェントの通信を制御します。

## kairo-win-adapter.exe

Windows ユーザーランドで KAIRO コアを稼働させ、パケット検査をシミュレーションまたは実行するためのアダプターです。

### 基本コマンド

- `kairo-win-adapter.exe status [--adapter <ID>]`
  指定したアダプターのステータスを確認し、コアへのアタッチをテストします。
  デフォルトのアダプター ID: `vEthernet-0`

- `kairo-win-adapter.exe inspect --data <HEX> [--adapter <ID>]`
  16進数エンコードされたパケットデータを検査し、コアによる判定（Verdict）を表示します。

### グローバルオプション
- `-v, --verbose <LEVEL>`
  ログの出力レベルを指定します (error, warn, info, debug, trace)。

## 判定結果 (Verdicts)
KAIRO コアは検査したパケットに対して以下のいずれかの判定を下します。

- **Accept**: パケットの通過を許可します。
- **Drop**: パケットを破棄します（沈黙）。
- **Defer**: 判定を保留し、後続の処理に委ねます。
- **Quarantine**: 隔離対象としてマークします。

## 開発中の機能
- NDIS フィルタドライバによるリアルタイム・フィルタリング
- WFP (Windows Filtering Platform) との統合
- GUI インストーラーによる自動構成
