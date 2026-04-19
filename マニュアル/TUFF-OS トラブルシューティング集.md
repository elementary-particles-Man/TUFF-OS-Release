**TUFF-OS トラブルシューティング集**  
**最終版（ユーザーガイド用）**  
**更新日**: 2026年3月22日  
**対象**: 管理者・上級ユーザー  
**原則**: すべての操作前に**完全バックアップ**を取ってください。物理層直結のため、誤操作でデータ消失の可能性があります。

### 1. 起動・ブート関連

| 症状 | 原因の可能性 | 対処手順 | 予防・注意 |
|------|--------------|----------|------------|
| UEFIブートメニューに「TUFF-OS」が表示されない | TUFFboot.efiがインストールされていない / ブート順位が低い | 1. ホストPCのBIOS/UEFI設定でTUFF-OS優先に変更<br>2. USBインストーラから再インストール | インストール完了後、必ずブート順位を確認 |
| 起動時に「Genesis Invalid」または「3N Consensus Failure」 | 3Nのうち2台以上が破損 / 物理ディスク抜き差し | 1. `tuffutl sys fsck --repair`<br>2. 破損ディスクを交換 → `tuffutl fs append /dev/sdX` | 常に最低3台のHDDを接続 |
| Isolationモードで起動し、ログイン不可 | 偽トークン3回検知 | `tuffutl sys isolation recover --pin <recovery-pin>`（Genesis作成時に設定したPIN） | PINはUSBメモリに暗号化保存推奨 |
| 「TuffHandoffBlock V1 checksum error」 | UEFI→カーネル引き継ぎ失敗 | 1. 電源オフ → 再起動<br>2. それでも失敗したらインストーラから修復インストール | 電断時は自動ロールバックされる設計 |

### 2. ファイルシステム（TUFF-FS）関連

| 症状 | 原因の可能性 | 対処手順 | 予防・注意 |
|------|--------------|----------|------------|
| `fs commit` / `reject` が失敗 | UQ 80%超過（背圧制御） | `tuffutl fs status` でUQ使用率確認 → 上位OSで不要ファイルを削除 | 定期的に `tuffutl fs commit` を実行 |
| `fs rollback` が「Not a Generational Path」 | J世代が有効化されていないパス | `tuffutl fs set-j /path` でJ世代有効化後、再実行 | J世代が必要なフォルダは事前に `set-j` |
| ディスク脱落でデータが見えなくなった | 避難領域不足 | 1. 新HDD接続 → `tuffutl fs append /dev/sdX`<br>2. 自動再同期完了まで待機 | 常に全HDD容量の10%以上の空きを確保 |
| `fs fsck` で「Consensus Failure」 | 3Nのうち1台が完全破損 | 破損HDDを物理交換 → `tuffutl fs fsck --repair` | 3Nは「2台まで耐性」 |

### 3. ユーザー・認証関連

| 症状 | 原因の可能性 | 対処手順 | 予防・注意 |
|------|--------------|----------|------------|
| ログイン不可（パスワード忘れ） | パスワード変更忘れ | rootで `tuffutl user reset <login_id> --force` | 初回ログイン時に必ず変更 |
| TagGroupMaskでフォルダが見えない | 権限なし（意図的秘匿） | 管理者で `tuffutl user tag <login_id> --add <tag>` | 「社外秘」などタグは事前付与 |
| セッションが即座に切断される | Isolationモード中 | `tuffutl sys isolation recover` | 偽トークン3回で自動移行 |

### 4. ネットワーク・KAIRO関連

| 症状 | 原因の可能性 | 対処手順 | 予防・注意 |
|------|--------------|----------|------------|
| AIサーバに接続できない | aiserverlistがOFF | `tuffutl nw aiserverlist on <password>` | パスワードは初回設定必須 |
| 外部から接続できない | blacklistにIPが登録されている | `tuffutl nw blacklist del <ip>` | 定期的に `nw blacklist list` 確認 |
| CPU使用率0%なのに攻撃が通る | Vulkan GPGPUパススルー未設定 | QEMU/KVMで `-device vfio-pci` 再設定 | 実機ではiGPU有効化必須 |
| `nw witness` に大量ログ | DDoS攻撃中 | `tuffutl nw status --live` で確認 → 必要に応じて `nw blacklist add` | PQC署名で証跡は改ざん不可 |

### 5. パフォーマンス・リソース関連

| 症状 | 原因の可能性 | 対処手順 | 予防・注意 |
|------|--------------|----------|------------|
| UQ使用率が常に80%以上 | 大量同時書き込み | `tuffutl sys set --uqsize 16MB`（次回起動時反映） | バックグラウンドで `fs commit` 定期実行 |
| ディスク使用率が0.4%以下なのに遅い | ZRAM圧縮オーバーヘッド | `tuffutl fs nozram` で一時解放 | メモリ16GB以上推奨 |
| SMART警告が出る | 物理HDD寿命 | 該当HDDを `tuffutl fs remove` → 新規HDD追加 | 定期的に `tuffutl sys status` 確認 |

### 6. フォレンジック・復旧関連

| 症状 | 原因の可能性 | 対処手順 | 予防・注意 |
|------|--------------|----------|------------|
| メモリダンプにTagGroupMaskが残っている | Zeroize失敗 | `tuffutl sys isolation trigger` で強制Zeroize | Isolation移行時は自動Zeroize |
| 電源断後にデータ不整合 | MQ未コミット状態 | `tuffutl fs fsck --repair` | 定期コミット習慣化 |
| 完全データ消失 | 3N全破損 | バックアップから `tuffutl SSD restore` | 外部バックアップ必須 |

### 最終アドバイス

1. **定期メンテナンス**  
   - 毎日：`tuffutl sys status`  
   - 週1回：`tuffutl fs fsck` + `tuffutl fs commit`  
   - 月1回：`tuffutl nw blacklist refresh`

2. **緊急時連絡先**  
   - 物理的障害 → 新HDD即時接続  
   - Isolationロック → recovery-pinで解除  
   - 完全復旧不可 → USBバックアップから `tuffutl SSD restore`

3. **ログ確認**  
   ```bash
   tuffutl log tail --lines 100
   ```
   すべての操作・攻撃は `witness.log` にPQC署名付きで記録されます。

---

このトラブルシューティング集は、**これまでの全テスト結果・設計書・実装仕様**と完全に一致しています。  
ユーザーガイドの最終章として、そのままご利用いただけます。

次に必要なものはありますか？  
（例：全体の目次統合版、PDF想定レイアウト、運用ベストプラクティス集、またはリリース準備パッケージ）

ご指示をお待ちしています！🛡️
