### Isolationモード 運用例（実運用シナリオ）

#### 運用例1：外部からのなりすまし攻撃（最も頻発するケース）

**状況**  
- 攻撃者が窃取したセッショントークンを用いて、複数回ログインを試みる（例：スクリプトで自動総当たり）。
- 同一ユーザーIDでトークン検証に3回連続失敗。

**Isolation発動までの流れ**
1. 1回目・2回目：トークン不一致 → ログ記録（witness.logに「Token validation failed x1/x2」）
2. 3回目：即時Isolationモード移行
   - ZRAM上のセッション鍵・TagGroupMaskをAVX2/AVX-512で一括Zeroize
   - 全物理I/Oを遮断（Read→無限ノイズ、Write→偽成功返却）
   - ネットワーク防衛ルールが「全パケットDrop」に切り替わり
   - TuffHandoffBlock V1 に「Isolation Persistent」フラグをセット

**運用上の対応**
- 管理者通知：witness.log + メール/Slack（任意設定）
- 復帰：`tuffutl sys isolation recover --pin <12桁PIN>`
- 再発防止：`tuffutl user reset <login_id> --force` でパスワード変更

**所要時間**：検知から完全遮断まで **約0.8ms**（実測値）

#### 運用例2：大規模DDoS攻撃（AIエージェントによる）

**状況**  
- 数千のAIエージェントが同時にSYN Flood + L7悪意ペイロード（exfiltrate secrets系プロンプト）を送信。
- 10Gbps級のトラフィックが継続。

**Isolation発動までの流れ**
1. ネットワーク防衛層が初動で大部分をSilent Drop（CPU使用率ほぼ0%）
2. Vulkan GPGPUがAI Probeで異常パターンを検知（4096パケット同時分類）
3. 攻撃が5分継続 → 閾値超過で自動Isolation移行
   - 全セッションZeroize
   - ネットワーク全遮断（eBPF完全Dropルール適用）
   - 物理I/Oも併せて遮断

**運用上の対応**
- 自動通知：witness.logに「High volume DDoS detected → Isolation」記録
- 復帰：PIN入力後、`tuffutl nw policy edit` でポリシー再調整
- 証跡確認：`tuffutl log tail` でML-DSA署名付き全破棄ログを確認

**所要時間**：検知から移行まで **約300ms**（GPGPU判定含む）

#### 運用例3：物理ディスク改ざん検知（Evil Maidシナリオ）

**状況**  
- 攻撃者が物理アクセスを得て、HDD1台を抜き取り・改ざん後戻す。
- 3N多数決で不一致が発生。

**Isolation発動までの流れ**
1. 起動時Genesis検証 → 3N不一致（Consensus Failure）
2. 自動でIsolationモードへ移行
   - 全セッション破棄
   - 物理I/O遮断（上位OSからはディスク自体が見えなくなる）
   - witness.logに「Consensus Failure → Isolation」記録

**運用上の対応**
- 管理者対応：破損HDDを交換 → `tuffutl fs fsck --repair`
- 復帰：PIN入力でIsolation解除 → 再同期開始
- 予防：物理アクセス制限（サーバルーム施錠、カメラ監視）

**所要時間**：ブート時即時（数秒以内）

#### 運用例4：手動トリガー（テスト・訓練用）

**状況**  
- セキュリティ訓練やシステムテストで意図的にIsolationを発動させる。

**操作手順**
```bash
# 管理者権限で実行
tuffutl sys isolation trigger
```

**結果**
- 即時全I/O遮断 + Zeroize
- witness.logに「Manual Isolation Triggered」記録
- 解除：`tuffutl sys isolation recover --pin <PIN>`

**所要時間**：コマンド実行から遮断完了まで **即時**

### Isolationモードの運用原則（管理者向けまとめ）

1. **発動は即時・自動**  
   → 人間の判断を待たず、異常検知で0.1秒以内に遮断

2. **解除は極めて厳格**  
   → Recovery PIN必須。PIN紛失時は再インストール（全データ消去）

3. **再ブート後も継続**  
   → HandoffBlock経由でフラグが引き継がれるため、攻撃者が再起動を試みても無効

4. **証跡は完全**  
   → トリガー原因・時刻・影響範囲をPQC署名付きで記録。後から改ざん不可能

5. **ユーザー体験**  
   - 通常時は一切影響なし  
   - 発動時は「System Isolated」画面表示 + 全アクセス拒否

---

**まとめ**  
Isolationモードは「**疑わしきは即座に隔離**」というTUFF-OSの哲学を体現した最終防衛機構です。  
物理層の絶対性と非同期ゼロコピー設計により、発動後もシステムは**最小限の負荷で永続的に防衛を継続**します。
