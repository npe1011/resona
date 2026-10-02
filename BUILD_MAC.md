# Resona macOS ビルドガイド

本ドキュメントでは、macOS 環境（Intel Mac / Apple Silicon M1, M2, M3, M4）で Resona をシングルバイナリとしてビルドする方法、および macOS ネイティブの `.app` アプリケーションバンドルを作成する手順について解説します。

---

## 1. 必要な前提環境

ビルドを開始する前に、以下の開発ツールがインストールされていることを確認してください。

### 1-1. Xcode Command Line Tools
ターミナルを開き、以下のコマンドを実行してインストールします（未導入の場合ダイアログが表示されます）：
```bash
xcode-select --install
```

### 1-2. Rust / Cargo
Rust 公式のインストーラ `rustup` を使用してインストールします：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

インストール後、バージョンを確認します：
```bash
rustc --version
cargo --version
```

---

## 2. シングルバイナリのビルド手順

まずはコマンドライン上で動作する単一の実行可能バイナリをビルドします。

### 開発ビルド (デバッグ)
```bash
cargo build
./target/debug/resona
```

### リリースビルド (最適化済みシングルバイナリ)
```bash
cargo build --release
```
- ビルドが完了すると、`target/release/resona` に単一の実行可能バイナリが生成されます。
- このバイナリは依存ライブラリを静的リンクしており、単独で動作します。
- アイコン画像リソース（PNG）もバイナリ内部にコンパイル時埋め込みされているため、ウィンドウ左上のアプリアイコンも自動的に表示されます。

ターミナルから直接ファイルパスを指定して起動することも可能です：
```bash
./target/release/resona /path/to/data.jdf
```

---

## 3. macOS アプリケーションバンドル (`Resona.app`) の作成

Finder や Launchpad からダブルクリックで起動でき、Dock に専用アイコンが表示される macOS ネイティブの `.app` バンドルを自動作成します。

プロジェクトルートに配置されている [build_mac.sh](file:///d:/work/resona/build_mac.sh) スクリプトを実行します：

```bash
chmod +x build_mac.sh
./build_mac.sh
```

### 生成される構成
スクリプトが成功すると、`target/bundle/osx/Resona.app` が生成されます：

```text
Resona.app/
  └── Contents/
      ├── Info.plist            # アプリケーションのメタデータ・対応拡張子 (.rsn, .jdf)
      ├── PkgInfo               # APPL????
      ├── MacOS/
      │   └── resona            # リリースビルドバイナリ (実行ファイル)
      └── Resources/
          └── icon.icns         # 高解像度 macOS アイコン (Retina対応 1024x1024)
```

### 動作確認
Finder またはターミナルから以下のコマンドで起動できます：
```bash
open target/bundle/osx/Resona.app
```

`/Applications` フォルダへ移動させれば、他の Mac アプリと同様に通常利用できます：
```bash
cp -R target/bundle/osx/Resona.app /Applications/
```

---

## 4. トラブルシューティング & 注意事項

### Apple Silicon (M1/M2/M3) と Intel Mac
- Mac 上で `cargo build` を実行すると、そのマシンのネイティブアーキテクチャ（Apple Silicon なら `aarch64-apple-darwin`、Intel なら `x86_64-apple-darwin`）向けのバイナリが自動生成されます。
- Universal Binary（両対応バイナリ）を作成したい場合は、以下の手順で作成可能です：
  ```bash
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/release-universal
  lipo -create \
    target/aarch64-apple-darwin/release/resona \
    target/x86_64-apple-darwin/release/resona \
    -output target/release-universal/resona
  ```

### 初回起動時の Gatekeeper 警告
自身でビルドした `.app` を起動する際、macOS のセキュリティ機能により「開発元を検証できないため開けません」と警告される場合があります。
- `build_mac.sh` 内でローカル用のアドホックコード署名（`codesign --sign -`）を実施しているため、通常のローカル実行ではブロックされにくくなっています。
- もし警告が出た場合は、**Finder で `Resona.app` を右クリック（または Control キーを押しながらクリック）し、「開く」を選択** してください。一度許可すれば次回以降は通常起動できます。
