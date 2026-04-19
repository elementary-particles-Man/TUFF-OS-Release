**TUFF-OS 詳細説明書（技術的説明書）**  
**最終版（視覚強化・Mermaid図完全挿入済み）**

---

## 1. アーキテクチャ概要

TUFF-OSは、上位OS（Windows / Linux / macOS等）の**下位レイヤー**で稼働するセキュリティ基盤OSです。  
論理的なファイルシステムの脆弱性を排除し、**物理セクタ（LBA）への直接アクセス**と**数学的暗号（KEY-CSE）**を組み合わせることで、「絶対防衛圏」を構築します。

```mermaid
flowchart TD
    A[物理層\nLBA直結\nGenesis / 3N冗長] 
    --> B[非同期ランタイム\nランタイム管理型スケジューラ]
    B --> C[ストレージ管理\nTUFF-FS\nUQ / HWキュー / N冗長 / J世代]
    C --> D[セキュリティ層\nDeception / Isolation / TagGroupMask / KAIRO]
    D --> E[上位OS\nWindows / Linux / macOS]

    classDef phys fill:#1e3a8a,color:#fff,stroke:#60a5fa,stroke-width:3px
    classDef runtime fill:#166534,color:#fff,stroke:#4ade80,stroke-width:3px
    classDef fs fill:#854d0e,color:#fff,stroke:#fbbf24,stroke-width:3px
    classDef sec fill:#9f1239,color:#fff,stroke:#fb7185,stroke-width:3px
    classDef upper fill:#1e40af,color:#fff,stroke:#93c5fd,stroke-width:3px

    class A phys
    class B runtime
    class C fs
    class D sec
    class E upper
```

---

## 2. ストレージ・サブシステム詳細

### 2.1 ブロックデバイスとLBA拘束

上位OSからは「JBOD（単一の巨大な仮想ドライブ）」として認識されますが、TUFF-OS内部では**各物理HDDのLBAを直接管理**しています。メタデータによる論理構造を持たないため、ファイルテーブルの改ざんや論理的フォレンジックが**物理的に不可能**です。

### 2.2 UQ (Unique Queue) と HWキューのメカニズム

```mermaid
flowchart LR
    A[上位OS I/O要求] --> B[UQ\nUnique Queue\nZRAM圧縮 + KEY-CSE暗号化]
    B --> C[背圧制御\n80%到達でI/Oブロック]
    C --> D[HWキュー\n動的割り当て]
    D --> E[物理HDD群\nシーケンシャルライト\nRead優先制御]
    E --> F[避難領域\n10%常時確保]

    classDef queue fill:#1e40af,color:#fff,stroke:#60a5fa
    classDef crypto fill:#166534,color:#fff,stroke:#4ade80
    classDef hw fill:#9f1239,color:#fff,stroke:#fb7185

    class B,C queue
    class D,E hw
    class F crypto
```

- **UQ (単一キュー)**: 上位OSからの全ライト要求を受け止めるバッファ領域。ZRAM上でのデータ圧縮およびKEY-CSE暗号化が施されます。
- **背圧制御 (Backpressure)**: UQ領域が設定値（デフォルト80%）に達すると、上位OSに対し安全にI/Oブロック信号を送信し、システムダウンを防ぎます。
- **HWキューとディスパッチ**: 暗号化済みのデータは、各物理HDDごとのHWキューに分配されます。最もI/O負荷の低いHDDが動的に選択され、ヘッドのシーク待ちを最小化するようシーケンシャルに書き込まれます。
- **リード優先制御**: リード要求が入った場合、進行中のライト処理を一時中断（Suspend）し、リードを優先させることでディスクの物理的損耗を防ぎます。

### 2.3 データ保護層 (N冗長 / J世代)

```mermaid
flowchart TD
    A[書き込み開始] --> B{N冗長\n1〜3複製}
    B --> C[コミット/リジェクト\nMQロールバック]
    C --> D[J世代\nEpoch Journaling]
    D --> E[ランサムウェア対策\nポインタ切替で即時復元]
```

- **N冗長 (1〜3の複製)**: 物理ディスクを跨いでデータを複製します。トランザクション管理により、書き込み中の電源断時でもMessage Queue (MQ)を用いたロールバックが作動し、1ビットのデータロストも防ぎます。
- **J世代 (Epoch Journaling)**: 更新ごとに元のLBAを上書きせず、新しいエポックとして別LBAへ書き込みます。これにより、ランサムウェア等による暗号化攻撃を受けた場合でも、インデックスのポインタを切り替えるだけで**一瞬で過去の世代へ復元**可能です。

### 2.4 緊急避難領域 (Emergency Area) とリビルド

全HDD容量の10%（既定値）を避難領域として確保します。ディスク障害の兆候（SMARTエラー等）を検知した場合、該当ディスクのデータを他の健全なディスクの避難領域へバックグラウンドで退避させます。  
新品のHDDをホットアタッチすると、避難領域のデータが新HDDへ自動で再同期（Append）され、**無停止でのリビルド**が完了します。

---

## 3. 非同期ランタイムとメモリ管理

### 3.1 ランタイム管理型非同期ランタイム

```mermaid
flowchart TD
    A[Genesis Block\n信頼の起点\nHW-ID刻印] --> B[3N Majority Vote\n3ディスク同期]
    B --> C[起動時検証\n2/3以上一致で真実判定]
    C --> D[破損自動修復\n1ディスク破損まで耐性]
    D --> E[3ディスク全破損\nFail-Closed即停止]

    classDef genesis fill:#1e3a8a,color:#fff,stroke:#60a5fa
    classDef vote fill:#166534,color:#fff,stroke:#4ade80
    classDef verify fill:#854d0e,color:#fff,stroke:#fbbf24
    classDef repair fill:#9f1239,color:#fff,stroke:#fb7185
    classDef fail fill:#991b1b,color:#fff,stroke:#fca5a5

    class A genesis
    class B vote
    class C verify
    class D repair
    class E fail
```
TUFF-Coreの非同期処理は、ホットパスでランタイム管理型のスケジューリングと固定長のセッション/状態構造を用います。タスクの起床（Wake）は `AtomicU32` のビットマップ通知によって行われ、割り込み（IRQ）からO(1)の定数時間でタスクを再開します。これにより、コアは応答性を保ちながら、キューイングと writeback 層で大量処理をさばけます。

### 3.2 ZRAMとSIMD Zeroize

セッション情報や権限タグ（TagGroupMask）は全てZRAMに展開されます。Isolation（隔離）モードへの移行時やログアウト時には、**AVX2 / AVX-512の256/512-bit SIMDストア命令**を用いて、メモリ上の機密データをコンマ数ミリ秒で一括消去（Zeroize）します。

---

## 4. セキュリティとネットワーク防衛

### 4.1 物理デセプション (ChaCha20 Read Deception)

未認証状態のまま物理ディスクを直接読み取ろうとする行為に対し、LBA位相とハードウェアIDをシードとしたChaCha20ストリーム暗号による「一貫性のあるノイズ」を返却します。AVX2の8レーン並列処理により、CPU負荷をかけずに無限のノイズを生成し続けます。

### 4.2 KEY-CSE 独自暗号

総鍵長768ビットの独自ストリーム暗号を採用しています。USBメモリ等の外部デバイスに格納された物理鍵と、ユーザーの認証トークンが揃わなければ復号は数学的に不可能です。

### 4.3 ネットワーク防衛網 (KAIRO-P)

- **eBPF (LSM/XDP) インターセプト**: カーネル空間でパケットを監視し、未承認ポートや不正な接続をOS到達前にSilent Drop（破棄）します。
- **Vulkan GPGPU オフロード**: 大規模なDDoS（SYN Flood等）やL7ペイロード解析（IDPI）をiGPUへオフロードし、CPU使用率0.0%のまま攻撃を無効化します。
- **PQC（ポスト量子暗号）証跡**: 破棄したパケットのログは、ML-DSA系列の量子耐性署名によってハッシュチェーン化され、改ざん検知可能な証跡として保存されます。

---

## 5. ブートプロセスとハンドオフ

### 5.1 TuffHandoffBlock V1

```mermaid
flowchart TD
    A[UEFIブート\nTUFFboot.efi] --> B[Genesis検証\n3N多数決]
    B --> C[セッション確立\nTagGroupMask生成]
    C --> D[TuffHandoffBlock V1\n4096Bアライメント]
    D --> E[カーネル起動\nIsolationフラグ引き継ぎ]
    E --> F[物理アクセス完全制御]
```

UEFIフェーズで確立された認証セッションやセキュリティ状態は、4096バイトにアライメントされたHandoffブロックを通じてカーネルへ安全に引き継がれます。UEFI段階でIsolationが発動した場合、そのフラグも引き継がれ、OS起動後も物理的なアクセス遮断が継続します。
