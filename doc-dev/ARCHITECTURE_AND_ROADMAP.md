# Resona アーキテクチャ設計・実装ロードマップ合意書

本ドキュメントは、`/grill-me` インタビューを通じて合意された「Resona」のRust移植・再実装の設計方針およびロードマップを記録するものである。

---

## 1. 全体アーキテクチャ構成

ロジックとGUIを完全に分離するため、Cargoワークスペース構成を採用する。

```
resona/
├── Cargo.toml                  # ワークスペース定義
├── crates/
│   ├── resona-core/            # 信号処理・解析エンジン・ファイルI/O (GUI非依存)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── io/             # JDFパーサー, .rsn/.ez読み書き (ZIP/NPY)
│   │   │   ├── signal/         # デジタルフィルタ, 窓関数, FFT, 位相補正
│   │   │   ├── baseline/       # ALS (五重対角バンドコレスキー分解)
│   │   │   ├── autophase/      # ACME (2変数Nelder-Mead最適化)
│   │   │   ├── analysis/       # ピークピッキング, 積分, J-coupling
│   │   │   ├── project.rs      # プロジェクト状態管理, Undo/Redoスナップショット
│   │   │   └── lib.rs
│   │   └── tests/              # Python版との完全数値一致テスト
│   └── resona-gui/             # egui/eframe によるデスクトップGUI
│       ├── Cargo.toml
│       ├── src/
│       │   ├── app.rs          # eframe::App 実装, メインループ
│       │   ├── plot/           # 高速スペクトル描画コンポーネント (Painter)
│       │   ├── modes/          # 完全モード制ステートマシン (View, Zoom, Peak, Integrate, etc.)
│       │   ├── panels/         # コンテキストアクションパネル, サイドパネル
│       │   ├── multiview/      # Insetインセット拡大窓の描画・操作
│       │   ├── dialogs/        # FTプレビュー, Display設定, J-Coupling選択
│       │   ├── print/          # A4横PDF生成, 高解像度画像エクスポート
│       │   └── main.rs
├── test_data/                  # Carbon1.jdf, Proton1.jdf (読み取り専用)
├── hand_over_doc/              # 引き継ぎ仕様書群
└── doc-dev/                    # 開発ドキュメント
```

---

## 2. 採用ライブラリ・技術選定

| コンポーネント | 採用技術 / クレート | 選定理由・実装方針 |
| :--- | :--- | :--- |
| **多次元配列・NPY** | `ndarray`, `ndarray-npy`, `num-complex` | NumPy互換の配列操作と `.rsn` 内のNPYバイナリ入出力。 |
| **高速フーリエ変換** | `rustfft` | Pure Rustで最高速。`fftshift`（前後半スワップ）と組み合わせてNMR順序を再現。 |
| **バイナリパース** | `byteorder` | JEOL JDFのビッグエンディアン／リトルエンディアン構造体を型安全にパース。 |
| **ALSベースライン** | 自前五重対角バンドコレスキー分解 | メモリ $O(N)$・計算量 $O(N)$ のソルバーを自前実装し、SciPy/Python版と完全一致させる。 |
| **ACME自動位相補正** | 自前Nelder-Meadシンプレックス法 | 2変数 $(p_0, p_1)$ 専用の最適化器を自前実装し、SciPy `fmin` と完全一致の収束を保証。 |
| **GUIフレームワーク** | `egui` (`eframe`) | 即時モードによる状態同期バグの撲滅、波形・ドラッグ操作に最適、Pure Rustシングルバイナリ。 |
| **印刷・エクスポート** | `printpdf`, `image` | A4横PDF直接生成 ＋ 学会・論文用高解像度PNG画像エクスポート（外部依存なし）。 |
| **データソース抽象化** | `NmrDataSource` トレイト | JEOL JDFおよび将来のBruker形式を統一的に扱える抽象層。 |

---

## 3. 実装フェーズとマイルストーン

### Phase 1: `resona-core` 信号処理・JDFパーサー（先行実装）
1. `NmrDataSource` トレイトおよび JDFバイナリパーサーの実装。
2. デジタルフィルタ群遅延 $D$ 算出とフーリエシフト（Fractional Shift）の実装。
3. 窓関数（EM, GM）およびゼロフィリングの実装。
4. 前進FFT（`rustfft` + `fftshift`）と位相補正（度数法）、左右反転、PPM軸算出。
5. **検証**: `test_data/Carbon1.jdf` と `test_data/Proton1.jdf` を読み込み、Python版との最大相対誤差が $10^{-5}$ 以下であることを `cargo test` で検証。

### Phase 2: `resona-core` 解析エンジン & プロジェクトI/O
1. ACME 自動位相補正（エントロピー＋負値ペナルティ＋Nelder-Mead）。
2. ALS 自動ベースライン補正（正定値五重対角コレスキー分解）。
3. ピークピッキング（MADノイズ推定、極大、プロミネンス、最小間隔）。
4. 積分エンジン（台形公式、局所線形ベースライン、プロトン数スケーリング）。
5. J-coupling解析（パスカル多重線ツリー畳み込み、モデルスコアリング、0.01Hz除外）。
6. `.rsn` プロジェクトファイル入出力（ZIP + JSON + NPY、旧 `.ez` 互換）。

### Phase 3: `resona-gui` インタラクティブUI
1. `eframe` ウィンドウ基盤と JEOL Delta風レイアウト。
2. 高速スペクトルプロット描画（X軸反転、パン、ズーム、Homeキーリセット）。
3. 完全モード切替制ステートマシン（View, Zoom, Phase, Baseline, Reference, Peak, Integrate, Multiview, J-Coupling）。
4. 各専用アクションパネルおよびサイドパネル（メタデータ、J-coupling結果テーブル）。
5. Multiview（インセット拡大窓）の移動・リサイズ・重畳描画・整列。
6. 各種ダイアログ（FTリアルタイムプレビュー、Display設定、J-Coupling候補選択）。
7. スナップショット方式による完全な Undo/Redo。

### Phase 4: 印刷・設定永続化・仕上げ
1. A4横PDF生成および高解像度PNG画像エクスポート。
2. `~/.resona/settings.json` のロードと終了時のみの保存（実行中I/O遮断）。
3. ファイルD&D（カレントディレクトリ追従）および Save時の `{stem}.rsn` 自動プリセット。
4. 全キーボードショートカットの検証。
