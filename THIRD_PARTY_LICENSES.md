# Third-Party Licenses and Acknowledgements

This document lists third-party software, open source libraries, and scientific works referenced or utilized by **Resona**.

---

## 1. Referenced Scientific Software & Prior Art

Resona is an independent, clean-room implementation written in Rust for 1D NMR spectroscopy analysis.
The file format parsing rules and digital signal processing algorithms were informed by the following open-source scientific tools:

### nmrglue
- **Repository**: [https://github.com/jjhelmus/nmrglue](https://github.com/jjhelmus/nmrglue)
- **Authors**: Jonathan J. Helmus, Christopher P. Jaroniec, and nmrglue contributors
- **License**: BSD 3-Clause License
- **Reference in Resona**: Consulted for Bruker and JEOL JDF file structure definitions, digital filter group delay values, and FFT conventions. All processing algorithms in Resona are independently implemented in Rust without embedding or distributing Python nmrglue binaries or source code.

```
Copyright (c) 2010-2024, Jonathan J. Helmus
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### ssNake
- **Repository**: [https://github.com/smeerten/ssnake](https://github.com/smeerten/ssnake)
- **Authors**: Sjoerd G.J. van Meerten, W. Leo Meerts, Arno P.M. Kentgens, and ssNake contributors
- **License**: GNU General Public License v3.0 (GPL-3.0)
- **Reference in Resona**: Consulted for scientific insights regarding JEOL FIR digital filter preprocessing and group delay formulas. Resona does not copy, translate, or link any ssNake code or binaries. All algorithms in Resona are original implementations in Rust based on public mathematical formulations and NMR signal processing theory. Consequently, Resona is not a derivative work of ssNake and is released under the permissive MIT license.

---

## 2. Direct Rust Dependencies

| Crate | Version | License | Description / Purpose |
| :--- | :--- | :--- | :--- |
| [egui](https://github.com/emilk/egui) | 0.29 | MIT OR Apache-2.0 | Immediate mode GUI library |
| [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) | 0.29 | MIT OR Apache-2.0 | Application framework for egui |
| [ndarray](https://github.com/rust-ndarray/ndarray) | 0.16 | MIT OR Apache-2.0 | N-dimensional array processing |
| [ndarray-npy](https://github.com/jturner314/ndarray-npy) | 0.9 | MIT OR Apache-2.0 | Reading and writing NumPy .npy files |
| [rustfft](https://github.com/ejmahler/RustFFT) | 6.4 | MIT OR Apache-2.0 | High-performance Fast Fourier Transform (FFT) |
| [num-complex](https://github.com/rust-num/num-complex) | 0.4 | MIT OR Apache-2.0 | Complex number representations |
| [rfd](https://github.com/PolyMeilex/rfd) | 0.15 | MIT | Native OS file and directory dialogs |
| [serde](https://github.com/serde-rs/serde) | 1.0 | MIT OR Apache-2.0 | Serialization/deserialization framework |
| [serde_json](https://github.com/serde-rs/json) | 1.0 | MIT OR Apache-2.0 | JSON serialization (.rsn, .rsm) |
| [zip](https://github.com/zip-rs/zip2) | 2.4 | MIT | Reading and writing zip archives (.rsm multi-spec projects) |
| [image](https://github.com/image-rs/image) | 0.25 | MIT OR Apache-2.0 | PNG image decoding for window icons |
| [byteorder](https://github.com/BurntSushi/byteorder) | 1.5 | Unlicense OR MIT | Binary byte ordering for raw data files |
| [thiserror](https://github.com/dtolnay/thiserror) | 2.0 | MIT OR Apache-2.0 | Ergonomic error handling derive macros |
| [approx](https://github.com/brendanzab/approx) | 0.5 | Apache-2.0 | Approximate floating-point comparisons (dev) |

---

## 3. Complete Bill of Materials (All 152 Linked Crates)

All crates linked into the Resona executable use permissive licenses (MIT, Apache-2.0, BSD, ISC, BSL, Zlib, CC0, Unicode, and OFL). **No copyleft (GPL / AGPL / LGPL) libraries are linked.**

| Crate | Version | License | Repository |
| :--- | :--- | :--- | :--- |
| `ab_glyph` | 0.2.32 | Apache-2.0 | [https://github.com/alexheretic/ab-glyph](https://github.com/alexheretic/ab-glyph) |
| `ab_glyph_rasterizer` | 0.1.10 | Apache-2.0 | [https://github.com/alexheretic/ab-glyph](https://github.com/alexheretic/ab-glyph) |
| `accesskit` | 0.16.3 | MIT OR Apache-2.0 | [https://github.com/AccessKit/accesskit](https://github.com/AccessKit/accesskit) |
| `accesskit_consumer` | 0.24.3 | MIT OR Apache-2.0 | [https://github.com/AccessKit/accesskit](https://github.com/AccessKit/accesskit) |
| `accesskit_windows` | 0.23.2 | MIT OR Apache-2.0 | [https://github.com/AccessKit/accesskit](https://github.com/AccessKit/accesskit) |
| `accesskit_winit` | 0.22.4 | Apache-2.0 | [https://github.com/AccessKit/accesskit](https://github.com/AccessKit/accesskit) |
| `adler2` | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [https://github.com/oyvindln/adler2](https://github.com/oyvindln/adler2) |
| `ahash` | 0.8.12 | MIT OR Apache-2.0 | [https://github.com/tkaitchuck/ahash](https://github.com/tkaitchuck/ahash) |
| `approx` | 0.5.1 | Apache-2.0 | [https://github.com/brendanzab/approx](https://github.com/brendanzab/approx) |
| `arboard` | 3.6.1 | MIT OR Apache-2.0 | [https://github.com/1Password/arboard](https://github.com/1Password/arboard) |
| `arrayvec` | 0.7.8 | MIT OR Apache-2.0 | [https://github.com/bluss/arrayvec](https://github.com/bluss/arrayvec) |
| `autocfg` | 1.5.1 | Apache-2.0 OR MIT | [https://github.com/cuviper/autocfg](https://github.com/cuviper/autocfg) |
| `bitflags` | 2.13.2 | MIT OR Apache-2.0 | [https://github.com/bitflags/bitflags](https://github.com/bitflags/bitflags) |
| `bumpalo` | 3.20.3 | MIT OR Apache-2.0 | [https://github.com/fitzgen/bumpalo](https://github.com/fitzgen/bumpalo) |
| `bytemuck` | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [https://github.com/Lokathor/bytemuck](https://github.com/Lokathor/bytemuck) |
| `bytemuck_derive` | 1.12.1 | Zlib OR Apache-2.0 OR MIT | [https://github.com/Lokathor/bytemuck](https://github.com/Lokathor/bytemuck) |
| `byteorder` | 1.5.0 | Unlicense OR MIT | [https://github.com/BurntSushi/byteorder](https://github.com/BurntSushi/byteorder) |
| `byteorder-lite` | 0.1.0 | Unlicense OR MIT | [https://github.com/image-rs/byteorder-lite](https://github.com/image-rs/byteorder-lite) |
| `cfg-if` | 1.0.5 | MIT OR Apache-2.0 | [https://github.com/rust-lang/cfg-if](https://github.com/rust-lang/cfg-if) |
| `cfg_aliases` | 0.2.2 | MIT | [https://github.com/katharostech/cfg_aliases](https://github.com/katharostech/cfg_aliases) |
| `clipboard-win` | 5.4.1 | BSL-1.0 | [https://github.com/DoumanAsh/clipboard-win](https://github.com/DoumanAsh/clipboard-win) |
| `crc32fast` | 1.5.2 | MIT OR Apache-2.0 | [https://github.com/srijs/rust-crc32fast](https://github.com/srijs/rust-crc32fast) |
| `cursor-icon` | 1.2.0 | MIT OR Apache-2.0 OR Zlib | [https://github.com/rust-windowing/cursor-icon](https://github.com/rust-windowing/cursor-icon) |
| `displaydoc` | 0.2.7 | MIT OR Apache-2.0 | [https://github.com/yaahc/displaydoc](https://github.com/yaahc/displaydoc) |
| `document-features` | 0.2.12 | MIT OR Apache-2.0 | [https://github.com/slint-ui/document-features](https://github.com/slint-ui/document-features) |
| `dpi` | 0.1.2 | Apache-2.0 AND MIT | [https://github.com/rust-windowing/winit](https://github.com/rust-windowing/winit) |
| `ecolor` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui](https://github.com/emilk/egui) |
| `eframe` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui/tree/master/crates/eframe](https://github.com/emilk/egui/tree/master/crates/eframe) |
| `egui` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui](https://github.com/emilk/egui) |
| `egui-winit` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui/tree/master/crates/egui-winit](https://github.com/emilk/egui/tree/master/crates/egui-winit) |
| `egui_glow` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui/tree/master/crates/egui_glow](https://github.com/emilk/egui/tree/master/crates/egui_glow) |
| `emath` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui/tree/master/crates/emath](https://github.com/emilk/egui/tree/master/crates/emath) |
| `epaint` | 0.29.1 | MIT OR Apache-2.0 | [https://github.com/emilk/egui/tree/master/crates/epaint](https://github.com/emilk/egui/tree/master/crates/epaint) |
| `epaint_default_fonts` | 0.29.1 | (MIT OR Apache-2.0) AND OFL-1.1 AND LicenseRef-UFL-1.0 | [https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts](https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts) |
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT | [https://github.com/indexmap-rs/equivalent](https://github.com/indexmap-rs/equivalent) |
| `error-code` | 3.4.0 | BSL-1.0 | [https://github.com/DoumanAsh/error-code](https://github.com/DoumanAsh/error-code) |
| `fdeflate` | 0.3.7 | MIT OR Apache-2.0 | [https://github.com/image-rs/fdeflate](https://github.com/image-rs/fdeflate) |
| `flate2` | 1.1.10 | MIT OR Apache-2.0 | [https://github.com/rust-lang/flate2-rs](https://github.com/rust-lang/flate2-rs) |
| `form_urlencoded` | 1.2.2 | MIT OR Apache-2.0 | [https://github.com/servo/rust-url](https://github.com/servo/rust-url) |
| `gl_generator` | 0.14.0 | Apache-2.0 | [https://github.com/brendanzab/gl-rs/](https://github.com/brendanzab/gl-rs/) |
| `glow` | 0.14.2 | MIT OR Apache-2.0 OR Zlib | [https://github.com/grovesNL/glow](https://github.com/grovesNL/glow) |
| `glutin` | 0.32.3 | Apache-2.0 | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin-winit` | 0.5.0 | MIT | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin_egl_sys` | 0.7.1 | Apache-2.0 | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin_wgl_sys` | 0.6.1 | Apache-2.0 | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 | [https://github.com/rust-lang/hashbrown](https://github.com/rust-lang/hashbrown) |
| `icu_collections` | 2.3.0 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_locale_core` | 2.3.0 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_normalizer` | 2.3.0 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_normalizer_data` | 2.3.0 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_properties` | 2.3.0 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_properties_data` | 2.3.0 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_provider` | 2.3.1 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `idna` | 1.1.0 | MIT OR Apache-2.0 | [https://github.com/servo/rust-url/](https://github.com/servo/rust-url/) |
| `idna_adapter` | 1.2.2 | Apache-2.0 OR MIT | [https://github.com/hsivonen/idna_adapter](https://github.com/hsivonen/idna_adapter) |
| `image` | 0.25.10 | MIT OR Apache-2.0 | [https://github.com/image-rs/image](https://github.com/image-rs/image) |
| `immutable-chunkmap` | 2.1.4 | Apache-2.0 OR MIT | [https://github.com/estokes/immutable-chunkmap](https://github.com/estokes/immutable-chunkmap) |
| `indexmap` | 2.14.2 | Apache-2.0 OR MIT | [https://github.com/indexmap-rs/indexmap](https://github.com/indexmap-rs/indexmap) |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 | [https://github.com/dtolnay/itoa](https://github.com/dtolnay/itoa) |
| `khronos_api` | 3.1.0 | Apache-2.0 | [https://github.com/brendanzab/gl-rs/](https://github.com/brendanzab/gl-rs/) |
| `libloading` | 0.8.9 | ISC | [https://github.com/nagisa/rust_libloading/](https://github.com/nagisa/rust_libloading/) |
| `litemap` | 0.8.3 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `litrs` | 1.0.0 | MIT OR Apache-2.0 | [https://github.com/LukasKalbertodt/litrs](https://github.com/LukasKalbertodt/litrs) |
| `lock_api` | 0.4.14 | MIT OR Apache-2.0 | [https://github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| `log` | 0.4.34 | MIT OR Apache-2.0 | [https://github.com/rust-lang/log](https://github.com/rust-lang/log) |
| `matrixmultiply` | 0.3.11 | MIT/Apache-2.0 | [https://github.com/bluss/matrixmultiply/](https://github.com/bluss/matrixmultiply/) |
| `memchr` | 2.8.3 | Unlicense OR MIT | [https://github.com/BurntSushi/memchr](https://github.com/BurntSushi/memchr) |
| `memoffset` | 0.9.1 | MIT | [https://github.com/Gilnaa/memoffset](https://github.com/Gilnaa/memoffset) |
| `miniz_oxide` | 0.8.9 | MIT OR Zlib OR Apache-2.0 | [https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) |
| `moxcms` | 0.8.1 | BSD-3-Clause OR Apache-2.0 | [https://github.com/awxkee/moxcms.git](https://github.com/awxkee/moxcms.git) |
| `ndarray` | 0.16.1 | MIT OR Apache-2.0 | [https://github.com/rust-ndarray/ndarray](https://github.com/rust-ndarray/ndarray) |
| `ndarray-npy` | 0.9.1 | MIT OR Apache-2.0 | [https://github.com/jturner314/ndarray-npy](https://github.com/jturner314/ndarray-npy) |
| `nohash-hasher` | 0.2.0 | Apache-2.0 OR MIT | [https://github.com/paritytech/nohash-hasher](https://github.com/paritytech/nohash-hasher) |
| `num-bigint` | 0.4.8 | MIT OR Apache-2.0 | [https://github.com/rust-num/num-bigint](https://github.com/rust-num/num-bigint) |
| `num-complex` | 0.4.6 | MIT OR Apache-2.0 | [https://github.com/rust-num/num-complex](https://github.com/rust-num/num-complex) |
| `num-integer` | 0.1.47 | MIT OR Apache-2.0 | [https://github.com/rust-num/num-integer](https://github.com/rust-num/num-integer) |
| `num-traits` | 0.2.19 | MIT OR Apache-2.0 | [https://github.com/rust-num/num-traits](https://github.com/rust-num/num-traits) |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 | [https://github.com/matklad/once_cell](https://github.com/matklad/once_cell) |
| `owned_ttf_parser` | 0.25.1 | Apache-2.0 | [https://github.com/alexheretic/owned-ttf-parser](https://github.com/alexheretic/owned-ttf-parser) |
| `parking_lot` | 0.12.5 | MIT OR Apache-2.0 | [https://github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| `parking_lot_core` | 0.9.12 | MIT OR Apache-2.0 | [https://github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| `paste` | 1.0.15 | MIT OR Apache-2.0 | [https://github.com/dtolnay/paste](https://github.com/dtolnay/paste) |
| `percent-encoding` | 2.3.2 | MIT OR Apache-2.0 | [https://github.com/servo/rust-url/](https://github.com/servo/rust-url/) |
| `pest` | 2.9.2 | MIT OR Apache-2.0 | [https://github.com/pest-parser/pest](https://github.com/pest-parser/pest) |
| `pest_derive` | 2.9.2 | MIT OR Apache-2.0 | [https://github.com/pest-parser/pest](https://github.com/pest-parser/pest) |
| `pest_generator` | 2.9.2 | MIT OR Apache-2.0 | [https://github.com/pest-parser/pest](https://github.com/pest-parser/pest) |
| `pest_meta` | 2.9.2 | MIT OR Apache-2.0 | [https://github.com/pest-parser/pest](https://github.com/pest-parser/pest) |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT | [https://github.com/taiki-e/pin-project-lite](https://github.com/taiki-e/pin-project-lite) |
| `png` | 0.18.1 | MIT OR Apache-2.0 | [https://github.com/image-rs/image-png](https://github.com/image-rs/image-png) |
| `potential_utf` | 0.1.6 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `primal-check` | 0.3.4 | MIT OR Apache-2.0 | [https://github.com/huonw/primal](https://github.com/huonw/primal) |
| `proc-macro2` | 1.0.107 | MIT OR Apache-2.0 | [https://github.com/dtolnay/proc-macro2](https://github.com/dtolnay/proc-macro2) |
| `pxfm` | 0.1.30 | BSD-3-Clause OR Apache-2.0 | [https://github.com/awxkee/pxfm](https://github.com/awxkee/pxfm) |
| `py_literal` | 0.4.0 | MIT OR Apache-2.0 | [https://github.com/jturner314/py_literal](https://github.com/jturner314/py_literal) |
| `quote` | 1.0.47 | MIT OR Apache-2.0 | [https://github.com/dtolnay/quote](https://github.com/dtolnay/quote) |
| `raw-window-handle` | 0.6.2 | MIT OR Apache-2.0 OR Zlib | [https://github.com/rust-windowing/raw-window-handle](https://github.com/rust-windowing/raw-window-handle) |
| `rawpointer` | 0.2.1 | MIT/Apache-2.0 | [https://github.com/bluss/rawpointer/](https://github.com/bluss/rawpointer/) |
| `rfd` | 0.15.4 | MIT | [https://github.com/PolyMeilex/rfd](https://github.com/PolyMeilex/rfd) |
| `rustfft` | 6.4.1 | MIT OR Apache-2.0 | [https://github.com/ejmahler/RustFFT](https://github.com/ejmahler/RustFFT) |
| `scopeguard` | 1.2.0 | MIT OR Apache-2.0 | [https://github.com/bluss/scopeguard](https://github.com/bluss/scopeguard) |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | [https://github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| `serde_core` | 1.0.229 | MIT OR Apache-2.0 | [https://github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| `serde_derive` | 1.0.229 | MIT OR Apache-2.0 | [https://github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | [https://github.com/serde-rs/json](https://github.com/serde-rs/json) |
| `simd-adler32` | 0.3.10 | MIT | [https://github.com/mcountryman/simd-adler32](https://github.com/mcountryman/simd-adler32) |
| `smallvec` | 1.16.1 | MIT OR Apache-2.0 | [https://github.com/servo/rust-smallvec](https://github.com/servo/rust-smallvec) |
| `smol_str` | 0.2.2 | MIT OR Apache-2.0 | [https://github.com/rust-analyzer/smol_str](https://github.com/rust-analyzer/smol_str) |
| `stable_deref_trait` | 1.2.1 | MIT OR Apache-2.0 | [https://github.com/storyyeller/stable_deref_trait](https://github.com/storyyeller/stable_deref_trait) |
| `static_assertions` | 1.1.0 | MIT OR Apache-2.0 | [https://github.com/nvzqz/static-assertions-rs](https://github.com/nvzqz/static-assertions-rs) |
| `strength_reduce` | 0.2.4 | MIT OR Apache-2.0 | [http://github.com/ejmahler/strength_reduce](http://github.com/ejmahler/strength_reduce) |
| `syn` | 3.0.6 | MIT OR Apache-2.0 | [https://github.com/dtolnay/syn](https://github.com/dtolnay/syn) |
| `synstructure` | 0.14.0 | MIT | [https://github.com/mystor/synstructure](https://github.com/mystor/synstructure) |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | [https://github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| `thiserror-impl` | 2.0.21 | MIT OR Apache-2.0 | [https://github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| `tinystr` | 0.8.4 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `tracing` | 0.1.44 | MIT | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `tracing-core` | 0.1.36 | MIT | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `transpose` | 0.2.3 | MIT OR Apache-2.0 | [https://github.com/ejmahler/transpose](https://github.com/ejmahler/transpose) |
| `ttf-parser` | 0.25.1 | MIT OR Apache-2.0 | [https://github.com/harfbuzz/ttf-parser](https://github.com/harfbuzz/ttf-parser) |
| `ucd-trie` | 0.1.7 | MIT OR Apache-2.0 | [https://github.com/BurntSushi/ucd-generate](https://github.com/BurntSushi/ucd-generate) |
| `unicode-ident` | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [https://github.com/dtolnay/unicode-ident](https://github.com/dtolnay/unicode-ident) |
| `unicode-segmentation` | 1.13.3 | MIT OR Apache-2.0 | [https://github.com/unicode-rs/unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation) |
| `url` | 2.5.8 | MIT OR Apache-2.0 | [https://github.com/servo/rust-url](https://github.com/servo/rust-url) |
| `utf8_iter` | 1.0.4 | Apache-2.0 OR MIT | [https://github.com/hsivonen/utf8_iter](https://github.com/hsivonen/utf8_iter) |
| `version_check` | 0.9.5 | MIT/Apache-2.0 | [https://github.com/SergioBenitez/version_check](https://github.com/SergioBenitez/version_check) |
| `web-time` | 1.1.0 | MIT OR Apache-2.0 | [https://github.com/daxpedda/web-time](https://github.com/daxpedda/web-time) |
| `webbrowser` | 1.2.4 | MIT OR Apache-2.0 | [https://github.com/amodm/webbrowser-rs](https://github.com/amodm/webbrowser-rs) |
| `winapi` | 0.3.9 | MIT/Apache-2.0 | [https://github.com/retep998/winapi-rs](https://github.com/retep998/winapi-rs) |
| `windows` | 0.58.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-core` | 0.58.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-implement` | 0.58.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-interface` | 0.58.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-link` | 0.2.1 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-result` | 0.2.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-strings` | 0.1.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-sys` | 0.59.0 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-targets` | 0.52.6 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_x86_64_msvc` | 0.53.1 | MIT OR Apache-2.0 | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `winit` | 0.30.13 | Apache-2.0 | [https://github.com/rust-windowing/winit](https://github.com/rust-windowing/winit) |
| `writeable` | 0.6.4 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `xml-rs` | 0.8.29 | MIT | [https://github.com/kornelski/xml-rs](https://github.com/kornelski/xml-rs) |
| `yoke` | 0.8.3 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `yoke-derive` | 0.8.3 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerocopy` | 0.8.58 | BSD-2-Clause OR Apache-2.0 OR MIT | [https://github.com/google/zerocopy](https://github.com/google/zerocopy) |
| `zerofrom` | 0.1.8 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerofrom-derive` | 0.1.8 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerotrie` | 0.2.5 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerovec` | 0.11.8 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerovec-derive` | 0.11.6 | Unicode-3.0 | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zip` | 2.4.2 | MIT | [https://github.com/zip-rs/zip2.git](https://github.com/zip-rs/zip2.git) |
| `zmij` | 1.0.23 | MIT | [https://github.com/dtolnay/zmij](https://github.com/dtolnay/zmij) |
| `zopfli` | 0.8.3 | Apache-2.0 | [https://github.com/zopfli-rs/zopfli](https://github.com/zopfli-rs/zopfli) |

---

## 4. Standard License Texts

### MIT License
```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Apache License, Version 2.0
```
Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

### SIL Open Font License 1.1 (OFL-1.1)
Used by default fonts in `epaint_default_fonts`. Full text available at: http://scripts.sil.org/OFL

### Boost Software License 1.0 (BSL-1.0)
Full text available at: https://www.boost.org/LICENSE_1_0.txt

### Unicode License Agreement (Unicode-3.0)
Full text available at: https://www.unicode.org/license.txt
