# tuffutl コマンドリファレンス & バックエンド仕様 詳細資料 v1.1.0
**最終更新**: 2026年3月22日  
**対象**: TUFF-OS管理者、開発者、上級ユーザー

`tuffutl` は TUFF-OS の唯一の正規管理インターフェースです。  
物理層操作のすべてを仲介し、上位OSからの直接アクセスを完全に遮断します。

## 1. 起動形態と基本ルール

| モード        | 起動方法                       | 主な用途              | 注意点                |
| ---------- | -------------------------- | ----------------- | ------------------ |
| CUIモード     | `tuffutl --cui`            | オフライン・トラブルシューティング | VGAテキストモード常駐       |
| バックエンド/IPC | 常駐（initramfs / systemd）    | Web-UI / スクリプト経由  | ZRAM上で動作           |
| ワンショット     | `tuffutl <command> [args]` | 自動化・バッチ           | `--json` で機械可読出力推奨 |

**共通オプション**（全コマンドで使用可能）

| オプション     | 意味                                    | 例 |
|----------------|-----------------------------------------|----|
| `--json`       | JSON形式出力                            | `--json` |
| `--verbose` / `-v` | 詳細ログ出力                        | `-vv` で最大詳細 |
| `--dry-run`    | 実行せずシミュレーション                | 安全確認用 |
| `--help`       | ヘルプ表示                              | `tuffutl fs commit --help` |

## 2. 権限レベル凡例

- **root**：物理層全操作可能（インストール時作成の特権ユーザー）
- **admin**：rootが付与可能な管理者権限
- **user**：一般ユーザー（TagGroupMaskで制御）

## 3. コマンド一覧（カテゴリ別）

### 3.1 sys（システム管理） — root専用

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 戻り値 / エラー例 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|-------------------|
| `sys status`                 | `--detail` / `--json`                   | システム全体状態（Genesis, 3N, Isolation, Disk Pool, KAIRO）表示    | JSON / Text       |
| `sys cpuinfo`                | `--json`                                | CPUマイクロアーキテクチャ・SIMD/AVX-512/VAES対応状況                 | Text / JSON       |
| `sys reboot`                 | —                                       | 安全再起動（全セッションLogout + Isolationフラグ保存）              | 0 / ERR_BUSY      |
| `sys poweroff`               | —                                       | 安全シャットダウン                                                  | 0                 |
| `sys isolation status`       | `--json`                                | Isolationモード状態                                                  | Active / Inactive |
| `sys isolation trigger`      | —                                       | 手動Isolation移行（テスト用）                                       | 0                 |
| `sys isolation recover`      | `--pin <recovery-pin>`                  | Isolation解除（Genesis紐づけPIN必須）                               | 0 / ERR_PIN       |

### 3.2 user（ユーザー管理） — root / admin

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 権限 / エラー例 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|-----------------|
| `user add <login_id>`        | `--password <pw>`                       | 新規ユーザー作成（初回強制変更フラグON）                             | root            |
| `user del <login_id>`        | —                                       | ユーザー削除（関連セッション即破棄）                                 | root            |
| `user list`                  | `--all` / `--json`                      | 全ユーザー一覧（状態・TagGroupMask表示）                             | root/admin      |
| `user reset <login_id>`      | `--force`                               | パスワードリセット（ゼロ初期化 + 強制変更ON）                       | root/admin      |
| `user password <login_id>`   | `--new <newpw>`                         | パスワード変更（rootは他ユーザーも可）                               | ログイン中必須  |
| `user tag <login_id>`        | `--add <tag>` / `--remove <tag>`        | TagGroupMask編集                                                     | root/admin      |

### 3.3 fs（ファイルシステム管理） — root / フォルダ管理者

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 権限 / エラー例 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|-----------------|
| `fs status`                  | `--detail` / `--json`                   | TUFF-FS全体状態（N冗長、J世代、UQ使用率、避難領域）                  | —               |
| `fs commit`                  | `--target <path>`                       | 指定パス変更を物理確定（N冗長ポインタ置換）                          | Attribute Error |
| `fs reject`                  | `--target <path>`                       | 指定パス変更を破棄（直前コミット状態へ）                             | Attribute Error |
| `fs rollback <epoch>`        | `--target <path>`                       | 指定パスを指定世代へロールバック                                     | Not a Generational Path |
| `fs fsck`                    | `--repair` / `--json`                   | 整合性チェック＆自動修復                                             | Consensus Failure |
| `fs nozram`                  | —                                       | ZRAM強制フラッシュ（デバッグ用）                                    | —               |
| `fs tag add <path> <tag>`    | —                                       | セキュリティタグ付与                                                 | 権限不足        |
| `fs tag remove <path> <tag>` | —                                       | タグ削除                                                             | 権限不足        |
| `fs tag list <path>`         | `--json`                                | 指定パスのタグ一覧                                                   | —               |

**重要ルール**  
`rollback` は **J世代パス**（世代管理フォルダ）限定。  
n冗長領域に対して実行すると **Attribute Error: Not a Generational Path** を返却します。

### 3.4 nw（ネットワーク管理） — root / ネットワーク管理者

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 備考 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|------|
| `nw status`                  | `--live` / `--json`                     | KAIRO状態（blacklist, aiserverlist, eBPFルール）                    | — |
| `nw blacklist add <ip/cidr>` | `--reason <text>`                       | ブロックリスト追加                                                   | — |
| `nw blacklist del <ip/cidr>` | —                                       | ブロックリスト削除                                                   | — |
| `nw blacklist refresh`       | —                                       | 外部リスト同期                                                       | — |
| `nw aiserverlist add <url>`  | `--password <pw>`                       | 許可AIサーバ追加                                                     | — |
| `nw policy edit`             | —                                       | eBPF Rescue Allowlist編集（インタラクティブ）                        | — |

### 3.5 その他ユーティリティ

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 備考 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|------|
| `version`                    | —                                       | バージョン情報                                                       | — |
| `help`                       | `[command]`                             | ヘルプ（引数なしで全一覧）                                           | — |
| `log tail`                   | `--lines <n>`                           | witness.logリアルタイム表示                                          | — |
| `test harness`               | `--forge-token <n>` / `--stress <type>` | テスト用（トークン偽造、負荷生成など）                               | 開発・検証用 |

## 4. 戻り値とエラーコード（主なもの）

| コード | 意味                 | 典型例                  |
| --- | ------------------ | -------------------- |
| 0   | 成功                 | —                    |
| 1   | 汎用エラー              | 内部エラー                |
| 2   | 権限不足               | root専用コマンドを一般ユーザーで実行 |
| 3   | Attribute Error    | J世代パス以外でrollbackなど   |
| 4   | Not Found / Secret | 権限外アクセス（意図的に秘匿）      |
| 5   | Isolation Active   | Isolationモード中の操作     |
| 6   | Consensus Failure  | 3N多数決不一致             |
| 7   | Pin Required       | Isolation解除にPIN必要    |
| 8   | Resource Exhausted | UQ 80%背圧、ディスクフルなど    |

## 5. 実装上の重要ルール（管理者・開発者向け）

1. **n冗長領域とJ世代領域の属性分離**  
   - `rollback` は **J世代パス**（世代管理フォルダ）限定  
   - n冗長領域に対して実行すると **Attribute Error: Not a Generational Path** を返却

2. **バックエンドの属性判断**  
   - ターゲットパスの属性（J指定の有無）を FMC（File Metadata Cache）から即座に判断  
   - 不適切な操作（n冗長に対するrollbackなど）は即座に拒否

3. **エラー時の挙動**  
   - 物理層エラー（Consensus Failureなど）は **Isolation移行を提案**  
   - 論理エラー（Attribute Errorなど）は **操作を拒否** し、詳細をwitness.logに記録

4. **JSON出力時の構造**（`--json` 指定時）

```json
{
  "status": "success" | "error",
  "code": 0 | 1 | 2 | ...,
  "message": "詳細メッセージ",
  "data": { ... }
}
````
