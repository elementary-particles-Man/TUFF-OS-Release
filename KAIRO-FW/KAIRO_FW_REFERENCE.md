# KAIRO-FW (CUI) 詳細リファレンス

KAIRO-FW は、汎用 Linux 環境向けに最適化された AI プロキシ・ファイアウォールです。
オリジナルの KAIRO から倫理エンジンや特定の OS 依存を排除し、純粋なネットワークフィルタリング機能を提供します。

## 基本コマンド

### デーモン制御
- `kairo-fw status`
  デーモン（kairo-fw-daemon）の稼働状況を確認します。
- `kairo-fw on`
  ファイアウォールを有効化し、サービスを開始します（自動起動設定を含む）。
- `kairo-fw off`
  ファイアウォールを無効化し、サービスを停止します。
- `kairo-fw reload`
  設定ファイルを再読み込みするためにデーモンを再起動します。

### AI ホスト管理 (`ai`)
AI トラフィックとして識別し、プロキシ対象とするホストを管理します。
- `kairo-fw ai list`
  登録されている AI ホストの一覧を表示します。
- `kairo-fw ai add <hostname>`
  新しい AI ホストを追加します。
- `kairo-fw ai remove <hostname>`
  指定した AI ホストを削除します。
- `kairo-fw ai edit`
  システムエディタで直接リストを編集します。

### ブラックリスト管理 (`blacklist`)
完全に通信を遮断するホストを管理します。
- `kairo-fw blacklist list`
  遮断対象のホスト一覧を表示します。
- `kairo-fw blacklist add <hostname>`
  遮断対象を追加します。
- `kairo-fw blacklist remove <hostname>`
  遮断対象を削除します。
- `kairo-fw blacklist edit`
  システムエディタで直接リストを編集します。

## 設定ファイル
すべての設定は `/etc/kairo-fw/` ディレクトリに保存されます。
- `/etc/kairo-fw/ai_hosts.txt` : AI ホストのリスト（1行に1ホスト）
- `/etc/kairo-fw/blacklist.txt` : 遮断対象のリスト

## ログ
デーモンのログは標準の journald 経由で確認できます。
```bash
journalctl -u kairo-fw -f
```
