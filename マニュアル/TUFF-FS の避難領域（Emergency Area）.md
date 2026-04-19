以下は、**TUFF-FS の避難領域（Emergency Area）** を中心に、全体の空き容量管理・冗長化・障害耐性との関係を詳細に図解したものです。

避難領域は「**全HDDの10%を常時確保し、障害発生時に他の健全ディスクへ自動退避する**」という、TUFF-FSの耐障害性の中核機能です。

### 1. 避難領域の全体像（Mermaid図）


### TUFF-FS 避難領域の全体像

```mermaid
flowchart TD
    subgraph "各物理HDD（例: 2TB×3台）"
        direction TB
        
        subgraph HDD1
            D1[データ領域<br>1.8TB] --> E1[避難領域<br>200GB / 10%]
        end

        subgraph HDD2
            D2[データ領域<br>1.8TB] --> E2[避難領域<br>200GB / 10%]
        end

        subgraph HDD3
            D3[データ領域<br>1.8TB] --> E3[避難領域<br>200GB / 10%]
        end
    end

    subgraph "障害発生時（HDD2故障）"
        F[障害検知<br>SMART異常 / 応答不良] -->|自動退避開始| E1
        E1 -->|データ転送| E3
        E3 --> R[3N復旧完了<br>無停止運用継続]
    end

    subgraph "新HDD追加時"
        N[新HDD挿入<br>/dev/sdg] --> S[避難領域データ転送]
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
````
### 2. 避難領域の詳細動作フロー（ステップごと）

```mermaid
sequenceDiagram
    participant User as 上位OS
    participant FS as TUFF-FS
    participant Monitor as 監視スレッド
    participant Disk as 物理HDD群

    User->>FS: 大量書き込み要求
    FS->>Disk: 通常領域へ分散書き込み
    Monitor->>Disk: SMART/応答監視（常時）
    alt 障害検知（SMART異常 / タイムアウト）
        Monitor->>FS: HDD2 故障通知
        FS->>Disk: HDD2 の全データを避難領域へ退避開始
        Disk->>Disk: HDD1 / HDD3 / HDD4 の避難領域を使用
        FS->>User: バックグラウンド退避中（上位OS影響なし）
        Disk->>FS: 退避完了報告
        FS->>User: 3N復旧完了通知
    else 新HDD追加
        User->>FS: 新HDD /dev/sdg 接続通知
        FS->>Disk: 避難領域データ → 新HDDへ同期
        Disk->>FS: 同期完了
        FS->>User: 3N完全復旧完了
    end
```

### 表示イメージの説明（テキスト）
- 左側：3台のHDDそれぞれに「データ領域」と「避難領域（10%）」が存在
- 中央：HDD2が故障した場合、E1（HDD1の避難領域）からE3（HDD3の避難領域）へデータが自動退避
- 右側：新HDDを挿入すると避難領域のデータが同期され、3Nが完全復旧

### さらに詳細な避難領域の動作フロー（シーケンス図）

```mermaid
sequenceDiagram
    participant User as 上位OS
    participant FS as TUFF-FS
    participant Monitor as 監視スレッド
    participant Disk as 物理HDD群

    User->>FS: 書き込み要求
    FS->>Disk: 通常領域へ分散書き込み
    Monitor->>Disk: SMART/応答監視（常時）

    alt 障害検知（HDD2故障）
        Monitor->>FS: HDD2異常通知
        FS->>Disk: HDD2のデータを避難領域へ退避開始
        Disk->>Disk: HDD1 / HDD3の避難領域を使用
        FS->>User: バックグラウンド退避中（影響なし）
        Disk->>FS: 退避完了報告
        FS->>User: 3N復旧完了通知
    end

    alt 新HDD追加
        User->>FS: 新HDD接続通知
        FS->>Disk: 避難領域データ → 新HDDへ同期
        Disk->>FS: 同期完了
        FS->>User: 完全3N復旧完了
    end
````
### 3. 避難領域の主要ルールと仕様まとめ

| 項目                     | 仕様詳細                                                                 | 備考・メリット |
|--------------------------|--------------------------------------------------------------------------|----------------|
| **確保率**               | 全HDD容量の**10%**（デフォルト、設定変更可）                            | 最低限の退避容量を常に確保 |
| **配置**                 | 各HDDの末尾領域（LBA末尾から逆方向に確保）                              | シーケンシャルライトに最適 |
| **使用タイミング**       | 1台のHDDが故障/異常検知 → 他のHDDの避難領域を使用                      | 無停止で3N維持可能 |
| **再同期（リビルド）**   | 新HDD追加時、自動で避難領域データを転送 → 3N完全復旧                    | ホットスワップ対応 |
| **Isolationモード時**    | 避難領域への新規退避も停止（全予約ロック）                              | 最終防衛時の完全凍結 |
| **容量不足時**           | 避難領域が枯渇 → 新規書き込みを一時停止（UQ背圧と連動）                | データロスト防止 |
| **監視間隔**             | SMART値チェック：1分ごと<br>応答タイムアウト：5秒×3回で検知            | 早期発見・早期退避 |

### 4. 避難領域の運用上のポイント（管理者向け）

- **定期確認コマンド**  
  ```bash
  tuffutl fs status --detail | grep Emergency
  ```
  → 各HDDの避難領域使用率・空き容量を表示

- **強制退避テスト**（訓練用）  
  ```bash
  tuffutl fs emergency simulate --disk /dev/sdc
  ```
  → HDD1台を擬似故障扱い → 退避動作を確認（テスト環境推奨）

- **新HDD追加時の推奨**  
  容量が既存HDDと同等以上であること（避難領域不足を防ぐ）

- **避難領域枯渇時の警報**  
  使用率90%超でwitness.logに警告記録 + 上位OSに通知（任意設定）

### まとめ

TUFF-FSの避難領域は、**「物理ディスク1台が突然死んでも、他のHDDの空き領域を使って無停止で3Nを維持し続ける」**という、TUFF-OSの耐障害性の中核を担う仕組みです。

- 常時10%確保 → 退避バッファとして機能
- 障害検知 → 自動退避開始
- 新HDD追加 → 自動再同期で完全復旧

これにより、**物理ディスクの故障がシステム全体の停止につながらない**、というTUFF-OSの絶対的な強さが実現されています。
