# tuffutl コマンドリファレンス（ユーザー・管理者向け） v1.1.0
**最終更新**: 2026年3月22日  
**対象**: TUFF-OS管理者および上位OSから操作するユーザー

`tuffutl` は TUFF-OS の唯一の管理インターフェースです。  
物理層操作のすべてを仲介し、上位OSからの直接アクセスを完全に遮断します。

## 1. 起動形態

| モード               | コマンド例                              | 使用場面                              | 備考 |
|----------------------|-----------------------------------------|---------------------------------------|------|
| CUIモード            | `tuffutl --cui`                         | トラブルシューティング、オフライン時   | VGAテキストモード常駐 |
| バックエンド/IPCモード | 常駐（initramfs / systemd）             | 通常運用時（Web-UI / スクリプト経由） | ZRAM上で動作 |
| ワンショット実行     | `tuffutl sys status`                    | スクリプト・自動化                    | 推奨 |

## 2. 共通オプション

| オプション           | 意味                                    | デフォルト |
|----------------------|-----------------------------------------|------------|
| `--json`             | JSON形式で出力                          | なし       |
| `--verbose` / `-v`   | 詳細ログ出力                            | なし       |
| `--dry-run`          | 実行せずにシミュレーション              | なし       |
| `--help`             | ヘルプ表示                              | —          |

## 3. コマンド一覧（カテゴリ別）

### 3.1 sys（システム管理） — root専用

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 戻り値例 / エラー |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|-------------------|
| `sys status`                 | `--detail`                              | システム全体状態（Genesis, 3N, Isolation, Disk Pool, KAIRO）表示    | JSON / Text       |
| `sys cpuinfo`                | —                                       | CPUマイクロアーキテクチャ・SIMD/AVX対応状況                        | Text              |
| `sys reboot`                 | —                                       | 安全再起動（全セッションLogout + Isolationフラグ保存）              | OK / ERR_BUSY     |
| `sys poweroff`               | —                                       | 安全シャットダウン                                                  | OK                |
| `sys isolation status`       | —                                       | Isolationモード状態                                                 | Active / Inactive |
| `sys isolation trigger`      | —                                       | 手動Isolation移行（テスト用）                                       | OK                |
| `sys isolation recover`      | `--pin <recovery-pin>`                  | Isolation解除（Genesis紐づけPIN必須）                               | OK / ERR_PIN      |

### 3.2 user（ユーザー管理） — root / admin

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 権限 / 備考 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|-------------|
| `user add <login_id>`        | `--password <pw>`                       | 新規ユーザー作成（初回強制変更フラグON）                             | root        |
| `user del <login_id>`        | —                                       | ユーザー削除（関連セッション即破棄）                                 | root        |
| `user list`                  | `--all`                                 | 全ユーザー一覧（状態・TagGroupMask表示）                             | root/admin  |
| `user reset <login_id>`      | `--force`                               | パスワードリセット（ゼロ初期化 + 強制変更ON）                       | root/admin  |
| `user password <login_id>`   | `--new <newpw>`                         | パスワード変更（rootは他ユーザーも可）                               | ログイン中  |
| `user tag <login_id>`        | `--add <tag>` / `--remove <tag>`        | TagGroupMask編集                                                     | root/admin  |

### 3.3 fs（ファイルシステム管理） — root / フォルダ管理者

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 備考 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|------|
| `fs status`                  | `--detail`                              | TUFF-FS全体状態（N冗長、J世代、UQ使用率、避難領域）                  | — |
| `fs commit`                  | `--target <path>`                       | 指定パス変更を物理確定（N冗長ポインタ置換）                          | — |
| `fs reject`                  | `--target <path>`                       | 指定パス変更を破棄（直前コミット状態へ）                             | — |
| `fs rollback <epoch>`        | `--target <path>`                       | 指定パスを指定世代へロールバック                                     | J世代パス限定 |
| `fs fsck`                    | `--repair`                              | 整合性チェック＆自動修復                                             | — |
| `fs nozram`                  | —                                       | ZRAM強制フラッシュ（デバッグ用）                                    | — |
| `fs tag add <path> <tag>`    | —                                       | セキュリティタグ付与                                                 | — |
| `fs tag remove <path> <tag>` | —                                       | タグ削除                                                             | — |
| `fs tag list <path>`         | —                                       | 指定パスのタグ一覧                                                   | — |

**重要**: `rollback` は **J世代パス**（世代管理フォルダ）限定。n冗長領域に対して実行すると「Attribute Error: Not a Generational Path」を返却します。

### 3.4 nw（ネットワーク管理） — root / ネットワーク管理者

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 備考 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|------|
| `nw status`                  | `--live`                                | KAIRO状態（blacklist, aiserverlist, eBPFルール）                    | — |
| `nw blacklist add <ip/cidr>` | `--reason <text>`                       | ブロックリスト追加                                                   | — |
| `nw blacklist del <ip/cidr>` | —                                       | ブロックリスト削除                                                   | — |
| `nw blacklist refresh`       | —                                       | 外部リスト同期                                                       | — |
| `nw aiserverlist add <url>`  | `--password <pw>`                       | 許可AIサーバ追加                                                     | — |
| `nw policy edit`             | —                                       | eBPF Rescue Allowlist編集（インタラクティブ）                        | — |

### 3.5 その他

| コマンド                     | 引数 / オプション                       | 説明                                                                 | 備考 |
|------------------------------|-----------------------------------------|----------------------------------------------------------------------|------|
| `version`                    | —                                       | バージョン情報                                                       | — |
| `help`                       | `[command]`                             | ヘルプ（引数なしで全一覧）                                           | — |
| `log tail`                   | `--lines <n>`                           | witness.logリアルタイム表示                                          | — |
| `test harness`               | `--forge-token <n>` / `--stress <type>` | テスト用（トークン偽造、負荷生成など）                               | 開発・検証用 |

### 4. 戻り値とエラーコード（主なもの）

| コード | 意味                 | 状況例                  |
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

### 5. よく使うコマンド例

```bash
# システム状態確認（一番使う）
tuffutl sys status --detail

# 新規ユーザー作成
tuffutl user add alice --password "TempPass123!"

# 機密フォルダにタグ付与
tuffutl fs tag add /data/secret "社外秘"

# ネットワークブロック追加
tuffutl nw blacklist add 192.168.1.100 --reason "malicious"

# ロールバック（ランサムウェア対策）
tuffutl fs rollback 42 --target /data/project
```

**注意**: すべての操作は**物理層に直結**するため、誤操作によるデータ消失リスクがあります。  
**必ずバックアップを取得し、テスト環境で練習**してから本番運用してください。
