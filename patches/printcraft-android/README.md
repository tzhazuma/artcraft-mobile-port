# printcraft-android: putting PrintCraft on Android

Upstream PrintCraft (repo `storytold/printcraft`, crates `pdfcraft*`) has no mobile code. This is
the Android integration: a new L6 app crate (`apps/printcraft-android`) that reuses the desktop
engine and egui UI unchanged, a Gradle shell, and one seam in the shared UI crate plus the
call-site renames every upstream bump needs.

PrintCraft is a **pure-Rust** application: the PDF parser, content-stream interpreter, rasterizer
and text extractor are its own crates (hayro, vendored and patched upstream), and the font work is
Skrifa. There is **no PDFium, no C/C++ library and no native FFI to cross-compile** — the only
platform code in this port is the Java picker bridge. The start-up self-test proves it on the
device (see §6).

## 1. The things to copy in

```bash
cd <printcraft-clone>

# 1. the app crate (workspace members are `apps/*`, so the root Cargo.toml needs no change)
rsync -a ~/artcraft-mobile/patches/printcraft-android/ apps/printcraft-android/

# 2. the UI-crate patch — REQUIRED, see §3
patch -p1 < ~/artcraft-mobile/patches/printcraft-android/upstream/ui-egui.patch
cp ~/artcraft-mobile/patches/printcraft-android/upstream/crates/ui-egui/src/fd.rs crates/ui-egui/src/fd.rs

# 3. the OCR-crate patch — REQUIRED for a clean published binary, see §3.1
patch -p1 < ~/artcraft-mobile/patches/printcraft-android/upstream/ocr.patch
```

### 3.1 Why the OCR patch exists

`crates/ocr/src/lib.rs` builds a last-resort model-search path from
`env!("CARGO_MANIFEST_DIR")`. Cargo injects that as an **env-var string**, not as a debug-info
path, so `--remap-path-prefix` does **not** rewrite it: the published `.so` leaked
`/Users/<builder>/artcraft-mobile/printcraft/crates/ocr` verbatim (a `strings libmain.so` shows
it). One occurrence, but it is the builder's home directory.

The patch gates that lookup to `#[cfg(not(target_os = "android"))]`. On Android it could never
have succeeded anyway — an APK contains no source tree — so this is a correctness fix as much as
a privacy one. With it applied the Android `.so` contains zero `/Users/` strings; verify with:

```bash
strings apps/printcraft-android/jniLibs/arm64-v8a/libmain.so | grep -c /Users/   # must print 0
```

`apps/printcraft-android/android/` is the Gradle project; `apps/printcraft-android/jniLibs/` is
where cargo-ndk writes the `.so` (gitignored, and pointed at by
`sourceSets { main { jniLibs.srcDirs = ['../../jniLibs'] } }`).

Registering the layer for `cargo xtask ci` (optional, does not affect the build): add
`("android", 6)` to `LAYERS` in `xtask/src/main.rs`.

## 2. Build and install

```bash
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/28.2.13676358"
export ANDROID_HOME="$HOME/Library/Android/sdk"

# Rust (release; thin LTO + codegen-units 1, ~7 minutes cold). The remap matters: without it
# the shipped .so contains 1 227 `/Users/<builder>/...` strings (`strings libmain.so | grep -c`).
export RUSTFLAGS="--remap-path-prefix=$HOME=/build --remap-path-prefix=$PWD=/src"
cargo +stable ndk -t arm64-v8a -o apps/printcraft-android/jniLibs build --release -p printcraft-android
#  → apps/printcraft-android/jniLibs/arm64-v8a/libmain.so

# APK (Gradle wrapper 9.3.1; the distribution is already in ~/.gradle/wrapper)
cd apps/printcraft-android/android
./gradlew --no-daemon assembleRelease
#  → app/build/outputs/apk/release/app-release.apk

adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am start -n ai.storyteller.printcraft/.MainActivity
adb logcat | grep -Ei "printcraft|pdfcraft|SafBridge|wgpu|panic|AndroidRuntime"
```

Release only: a debug `.so` is ~850 MB. The release build type uses the debug signing config, so
`adb install -r` works on any device/emulator.

On this machine the harness that works is `~/port-scripts/fix-java-build.sh printcraft` (run from
the clone): it deletes Gradle's `javac` intermediates, which is what actually lands a new
`MainActivity.java`/`SafBridge.java` in the dex. Its final check looks for `safPickMedia`, the JNI
name the other ports use — PrintCraft's is `safPickDocuments`, so a **successful** build still
exits 1 and does not copy the APK; take `app/build/outputs/apk/release/app-release.apk` yourself.
`~/port-scripts/rebuild-so.sh` only knows `~/artcraft-android/src/<app>` and PrintCraft lives in
`~/artcraft-mobile/printcraft`, so run the cargo-ndk line above from the clone.

| Setting | Value |
|---|---|
| `namespace` / `applicationId` | `ai.storyteller.printcraft` |
| label | `PrintCraft` |
| `android.app.lib_name` | `main` (matches `[lib] name = "main"`) |
| minSdk / targetSdk / compileSdk | 31 / 35 / 35 |
| AGP / Gradle / Java | 9.1.0 / 9.3.1 / 17 |
| Activity | `GameActivity` from `androidx.games:games-activity:4.4.0` |

**GameActivity, not NativeActivity.** `accesskit` + `android-native-activity` is a hard eframe
compile error, and panel-heavy applications need GameActivity for the IME and proper input.

## 3. The upstream patch (`upstream/`)

PrintCraft is the only app of the five whose dialogs do **not** sit behind a host hook:
`crates/ui-egui` calls `rfd::FileDialog` / `rfd::AsyncFileDialog` directly at 32 sites, and it also
depends on `arboard`. Neither crate has an Android backend:

- **`rfd` 0.17.2 does not compile for `target_os = "android"` at all** — its platform traits
  (`FilePickerDialogImpl`, `FileSaveDialogImpl`, …) have no implementation there, so the errors come
  from inside rfd itself, not from our call sites.
- **`arboard` 3.6.1** has no Android backend either: `platform::Clipboard` does not exist for
  Android, so `arboard` fails to compile too.

So the port needs one seam in the shared UI crate. After the 2026-10-24 sync the patch is **550
lines of diff over 21 files: 73 insertions and 53 deletions, of which 47 lines are the mechanical
`rfd::` → `crate::fd::` renames** (the rest is context and the clipboard gates), plus one new
221-line file.

### 3.1 What the patch does

1. **New file `crates/ui-egui/src/fd.rs`** — the seam. On every platform `rfd` supports it is a
   re-export (`pub use rfd::{AsyncFileDialog, FileDialog, FileHandle}`); on Android it is a
   lookalike with the same builder API (`new`, `add_filter`, `set_title`, `set_file_name`,
   `set_directory`, `pick_file`, `pick_files`, `pick_folder`, `save_file`, and the async forms —
   upstream now uses the async forms for every picker, see §3.3) that asks a `Host` the app
   installs with `set_host`. The module documents why: an Android picker must be started on
   the UI thread and answered later, which a blocking dialog cannot do (the frame that would receive
   the answer is the frame it blocks).
2. **`mod fd;`** added to `crates/ui-egui/src/lib.rs` (next to `mod files;`).
3. **47 call sites renamed** in 20 files: `rfd::FileDialog` / `rfd::AsyncFileDialog` /
   `rfd::FileHandle` → `crate::fd::…`. This is purely mechanical and platform-neutral: on the
   desktop the types are still rfd's. The sync added four files with call sites of their own
   (`combine_ui.rs`, `fill_sign.rs`, `folders_ui.rs`, `image_pick.rs`), so the rename is not a
   one-off — see §3.2. The prose mentions of `rfd` in module docs were left alone deliberately.
4. **`crates/ui-egui/Cargo.toml`**: `rfd` and `arboard` moved from
   `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` to
   `[target.'cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))'.dependencies]`.
   `pollster` and `getrandom` stay (both are pure Rust and work on Android).
5. **Four `arboard` call sites gated**:
   - `create_ui.rs`: `read_clipboard()` is now `cfg(all(not(wasm32), not(android)))` with an
     Android twin that returns `None` (the app's own snapshot still pastes);
   - `zoom_snap.rs`: the `system_clipboard` write is `cfg(all(not(wasm32), not(android)))`;
   - `image_pick.rs`: `put_file_on_clipboard` writes the private file and skips the system
     clipboard on Android;
   - `content_ui.rs`: Copy Image keeps the app's own `last_copied_image` and skips the system
     clipboard on Android.

### 3.2 Re-applying after an upstream bump

The patch is a plain unified diff against the pinned upstream commit, so `patch -p1` (or
`git apply`) tells you immediately if a hunk moved. If upstream has renamed or added `rfd` call
sites, redo step 3 mechanically:

```bash
cd <printcraft-clone>/crates/ui-egui/src
grep -rn "rfd::" .                      # every remaining hit must become crate::fd::
sed -i '' 's/rfd::AsyncFileDialog/crate::fd::AsyncFileDialog/g; s/rfd::FileDialog/crate::fd::FileDialog/g; s/rfd::FileHandle/crate::fd::FileHandle/g' *.rs
```

(Do not rewrite the prose in `pickers.rs`'s `//!` header.) Then re-apply steps 1, 2, 4 and 5.

### 3.3 The 2026-10-24 sync (upstream `c894928e`, 2 202 commits)

The stash's 16 conflicting files under `crates/ui-egui` conflicted because upstream had rewritten
every native picker: the blocking `rfd::FileDialog` calls the old patch routed are gone, replaced
by `rfd::AsyncFileDialog` built in one place and answered through `pickers::ask` on a worker
thread. That is a better shape for Android — the old shim could not answer a blocking open at all
— so no conflict was resolved by keeping both sides:

- **Upstream won wherever it had rewritten the code**; all 47 surviving `rfd::` code sites were
  then re-routed to `crate::fd::` mechanically (including the four new files listed in §3.1).
- **The seam is still the port's**: upstream still has no Android backend for `rfd` or `arboard`,
  so `fd.rs`, `pub mod fd`, the `Cargo.toml` target gate, the four clipboard gates and the OCR
  patch survive unchanged in intent.
- `RenderRequest` gained `preview`; the start-up self-test passes `false` (it is a fidelity
  render, not a preview).
- `Cargo.lock` was taken from upstream and regenerated by cargo, never hand-merged.

Upstream's async rewrite also removes the old limitation that synchronous dialogs were inert on
Android: every dialog it opens now goes through `pickers::ask`, and on Android that future is the
SAF host. `FileDialog` (the blocking shim) is kept for API parity but no longer called.

### 3.4 Reusable beyond PrintCraft

`fd.rs`'s `Host` trait is deliberately app-neutral: `open` (returns real paths),
`save` (reserves a scratch path) and `folder`. If another Crafting App ever grows a dialog inside a
shared crate, the same seam applies — but **it is not shared infrastructure today**: filmcraft,
lightcraft and effectcraft keep `rfd` in their desktop entry crate, and photocraft keeps it (and
`arboard`) there too. Only PrintCraft's UI crate reaches for a dialog itself.

## 4. What the shell implements (`src/`)

| File | What it does |
|---|---|
| `lib.rs` | `android_main`, the `TouchShell`, the phone adjustments, the CJK fallback, the command palette |
| `saf.rs` | the Storage Access Framework as the `fd` host: JNI to `MainActivity`, pickers, the scratch-file publish |
| `selftest.rs` | start-up proof that the PDF stack works on the device |

- **File ▸ Open works through the app's own path.** `PdfCraftApp::open_dialog()` builds an
  *asynchronous* dialog and `pickers::Pickers` polls it on a worker thread; on Android that future
  is the SAF picker, so the picked document is copied into `<filesDir>/import/` by Java and opened
  by the app's own `process_picked` → `open_path` on a later frame. No dialog code was rewritten for
  Android and the UI thread never blocks.
- **Save/Save As/export work through a scratch file.** The engine writes plain paths, so
  `Host::save` answers with `<externalFilesDir>/exports/<suggested name>`, and a worker thread shows
  `ACTION_CREATE_DOCUMENT` and copies the finished file (size stable ≥1.5 s) to the destination the
  user chose. Cancelling loses nothing: the file stays in `exports/`.
- **The MIME list comes from the dialog's own filters** (`mimes_for`), so File ▸ Open offers PDFs,
  images and text, Combine offers PDFs, and the digital-ID dialog offers `application/x-pkcs12`.
  Passing an explicit list also matters on some OEM pickers (vivo), which otherwise hide everything
  but "documents".
- **The command palette** (long-press the top 9 % of the screen for 500 ms) gives the file, edit and
  export commands finger-sized buttons plus page/zoom controls. Every button runs the app's own
  registered command (`PdfCraftApp::execute`) or the same `DocView` call the desktop rail makes.
- **Phone start-up state**: Read mode with the left tool panel closed (`app.mode`, `app.left_open` —
  the desktop app's own fields), 40 pt hit targets, no UI scaling (see `TOUCH_SCALE`).
- **Storage**: settings in `<data>/app.ron` (eframe persistence), recovery snapshots in
  `<data>/recovery` (`RecoveryStore`), picked documents in `<data>/import/`, scratch exports in
  `<external>/exports/`.
- **CJK font**: the bundled Inter has no CJK glyphs and the craft-fonts build input is optional, so
  the device's Noto CJK face is appended to egui's *current* definitions — after the theme install
  (any earlier and egui panics on the theme's `FontFamily::Name("semibold")` having no binding), and
  re-applied if a language switch replaces the font set.

## 5. Known gaps

- **The system clipboard is not wired** (`arboard` has no Android backend). The four write/read
  sites are gated; the app's own snapshot (`Edit ▸ Paste` from `zoom_snap`) still works. The
  Android clipboard needs a `ClipboardManager` JNI bridge.
- **Folder questions** answer with the app's own `exports/` directory: SAF's tree picker returns a
  document tree the engine cannot write to by path (`ACTION_OPEN_DOCUMENT_TREE` is not wired).
- **Printing** (`pdfcraft-print`) spools through `lp`/`lpstat` on Unix, which does not exist on
  Android; the Print dialog is unreachable rather than broken. Android printing needs a
  `PrintManager`/`PdfRenderer`-style bridge.
- **Launcher icon**: the Gradle shell still carries the shared ArtCraft placeholder mipmaps.
- **`unsafe` policy**: this crate uses `unsafe_code = "deny"` with `#[allow(unsafe_code)]` on
  `android_main` (for `#[unsafe(no_mangle)]`) and on the three JNI helpers, each with a `// SAFETY:`
  note. If upstream takes this, ADR 0001 should be revised to name the Android entry crate the same
  way it names `crates/platform`.

## 6. The start-up self-test

`selftest.rs` embeds a one-page PDF written for the test (`assets/test.pdf`, 756 bytes: Letter,
Helvetica text, one filled rectangle — contributor-original, no third-party asset) and, on a
background thread at start-up, parses it (`pdfcraft_render::inspect`), rasterizes page 1 at 1:1
(`PageRenderer`) and extracts its text layer (`RequestKind::Text`). One logcat line carries the
verdict:

```
printcraft-android: SELFTEST ok: PDF 1.7 | page (612, 792) pt → raster 612×792 (12538 ink px of 484704, 3 ms) | fonts 1 | text 46 glyphs (expected "PrintCraft on Android") | parser+rasterizer+text: working
```

A blank page, a failed parse or a missing text layer is visible from `adb logcat` alone.

## 7. Verified

### 7.1 Emulator (API 36, arm64, SwiftShader)

Build and install:

```
cargo +stable ndk -t arm64-v8a -o apps/printcraft-android/jniLibs build --release -p printcraft-android
  → jniLibs/arm64-v8a/libmain.so            50,788,928 bytes
(cd apps/printcraft-android/android && ./gradlew --no-daemon assembleRelease)
  → app/build/outputs/apk/release/app-release.apk   48,731,622 bytes
  → dist/PrintCraft-0.2.1-android-arm64.apk         sha256 cbe6001bedc1f8055eba8a098681fb2fbb7a41956c2cd77163ee82867c647dea
```

The packaged `libmain.so` is stripped to 40.9 MB by the Android plugin, and its four LOAD segments are
`0x4000`-aligned (16 KB pages). The APK also carries three smaller cdylibs the Rust build produces
(`libboa_engine`, `libocrs`, `librten`).

Launch (logcat, tag `PrintCraft`):

```
I PrintCraft: printcraft-android: android_main entered (PdfCraft 0.2.1)
I PrintCraft: printcraft-android: export scratch directory /storage/emulated/0/Android/data/ai.storyteller.printcraft/files/exports
I PrintCraft: printcraft-android: SAF file dialogs installed (host_installed=true)
I PrintCraft: printcraft-android: SELFTEST ok: PDF 1.7 | page (612, 792) pt → raster 612×792 (59311 ink px of 484704, 1 ms) | fonts 1 | text 74 glyphs (expected "PrintCraft on Android") | parser+rasterizer+text: working
ActivityTaskManager: Displayed ai.storyteller.printcraft/.MainActivity for user 0: +1s623ms
I PrintCraft: printcraft-android: touch mode applied (ppp 2.625, logical 411×914 pt, interact 40 pt)
I PrintCraft: printcraft-android: loaded system CJK font /system/fonts/NotoSansCJK-Regular.ttc
```

File ▸ Open through the palette, end to end (a three-page test PDF pushed to Downloads):

```
I PrintCraft: palette: Open PDF… → file.open
I PrintCraft: main::saf: SAF: opening the picker (types application/pdf|image/png|image/jpeg|image/tiff|image/gif|image/bmp|image/jp2|text/plain, multiple false, title "")
I SafBridge:  copied content://com.android.providers.downloads.documents/document/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2Fprintcraft-demo.pdf -> /data/user/0/ai.storyteller.printcraft/files/import/printcraft-demo.pdf
I PrintCraft: main::saf: SAF: 1 document(s) picked
```

The document then renders in the app (`shots/printcraft-opened.png`: all three pages, the tool rail,
the page/zoom rail at 28 %). Save As reaches the system create-document dialog and, when it is
cancelled, keeps the file:

```
I PrintCraft: main::saf: SAF: save dialog → scratch /storage/emulated/0/Android/data/ai.storyteller.printcraft/files/exports/printcraft-demo.pdf (suggested printcraft-demo.pdf, type application/pdf)
I PrintCraft: main::saf: SAF: no destination chosen; the file stays at …/exports/printcraft-demo.pdf
I PrintCraft: printcraft-android: Save cancelled: the file stayed in the app's folder
```

Screenshots: `shots/printcraft-launch.png` (home), `shots/printcraft-palette.png` (command palette),
`shots/printcraft-opened.png` (the PDF open and rendering).

Two defects were found this way and fixed before the build above: the palette was drawn under
PrintCraft's own menus (it needs `egui::Order::Tooltip`), and the log tag defaulted to `main`
(the cdylib's name), which no logcat filter can use.

### 7.2 Real device (vivo PA2573, Android 16, Mali-G925, Vulkan)

`scripts/device-smoke.sh ai.storyteller.printcraft .MainActivity PrintCraft <device-ip>:37379 printcraft-real dist/PrintCraft-0.2.1-android-arm64.apk` installs, launches and screenshots:

```
I PrintCraft: printcraft-android: android_main entered (PdfCraft 0.2.1)
I PrintCraft: printcraft-android: SELFTEST ok: PDF 1.7 | page (612, 792) pt → raster 612×792 (59311 ink px of 484704, 4 ms) | fonts 1 | text 74 glyphs (expected "PrintCraft on Android") | parser+rasterizer+text: working
I PrintCraft: egui_wgpu: There are 2 available wgpu adapters: {backend: Vulkan, device_type: IntegratedGpu, name: "Mali-G925-Immortalis MC12", …}
I PrintCraft: printcraft-android: touch mode applied (ppp 2.5, logical 1238×826 pt, interact 40 pt)
I PrintCraft: printcraft-android: loaded system CJK font /system/fonts/NotoSansCJK-Regular.ttc
```

On that device the logical viewport is 1238×826 pt, so PrintCraft's desktop layout fits as it is
(`shots/printcraft-real.png`: the mode bar in **Read** mode, all five tool cards, the page).

The whole open → save round trip was driven through the system UI on the phone:

```
# Open: the palette's "Open PDF…" → vivo's document picker → Download/printcraft-demo.pdf
I PrintCraft: main::saf: SAF: opening the picker (types application/pdf|image/png|…, multiple false, title "")
I SafBridge:  copied content://com.android.providers.media.documents/document/document%3A1000026764 -> /data/user/0/ai.storyteller.printcraft/files/import/printcraft-demo.pdf
I PrintCraft: main::saf: SAF: 1 document(s) picked
# …the document renders (shots/printcraft-real-opened.png: page 1 of 3, 129 %)
# Save As… → the palette's own button → ACTION_CREATE_DOCUMENT (name pre-filled)
I PrintCraft: main::saf: SAF: save dialog → scratch /storage/emulated/0/Android/data/ai.storyteller.printcraft/files/exports/printcraft-demo.pdf (suggested printcraft-demo.pdf, type application/pdf)
I SafBridge:  save destination: content://com.android.providers.downloads.documents/document/314
I SafBridge:  published /storage/emulated/0/Android/data/…/exports/printcraft-demo.pdf -> content://…/document/314
I PrintCraft: main::saf: SAF: published /storage/emulated/0/Android/data/…/exports/printcraft-demo.pdf (4353 bytes) to the chosen destination
```

and the result is a real file on the device: `/sdcard/Download/printcraft-demo (1).pdf`, 4353 bytes
(the picker added "(1)" because the source was still there). Screenshot: `shots/printcraft-real-saved.png`.

### 7.3 After the sync to upstream `c894928e` (2026-10-24)

Built from `~/artcraft-mobile/printcraft` with the remapped release profile, then
`~/port-scripts/fix-java-build.sh printcraft` recompiled the Java so `SafBridge` lands in the dex.
(The script exits 1 on PrintCraft because its final check looks for `safPickMedia`; the build
itself succeeded and the APK was copied by hand.)

```
  libmain.so      66,008,832 bytes;  strings | grep -c /Users/ = 0
  app-release.apk 62,391,495 bytes;  debug-signed;  dex: safPickDocuments, safPickSave,
                                   safPublish, SafBridge
  packaged libmain.so           54,399,800 bytes
```

The old package was uninstalled first, then the new one installed:

```
$ adb shell dumpsys package ai.storyteller.printcraft | grep -E 'versionName|firstInstallTime'
    versionName=0.2.1
    firstInstallTime=2026-10-24 21:31:09
    lastUpdateTime=2026-10-24 21:31:09
```

Launch:

```
$ adb shell am start -W -n ai.storyteller.printcraft/.MainActivity
Status: ok   LaunchState: COLD   Activity: ai.storyteller.printcraft/.MainActivity   TotalTime: 153
$ adb shell dumpsys activity activities | grep topResumedActivity
    topResumedActivity=ActivityRecord{56194510 u0 ai.storyteller.printcraft/.MainActivity t1359 d0}
$ adb shell pidof ai.storyteller.printcraft
8430
I PrintCraft: printcraft-android: android_main entered (PdfCraft 0.13.0)
I PrintCraft: printcraft-android: SAF file dialogs installed (host_installed=true)
I PrintCraft: printcraft-android: SELFTEST ok: PDF 1.7 | page (612, 792) pt → raster 612×792 (59311 ink px of 484704, 3 ms) | fonts 1 | text 74 glyphs (expected "PrintCraft on Android") | parser+rasterizer+text: working
I PrintCraft: printcraft-android: loaded system CJK font /system/fonts/NotoSansCJK-Regular.ttc
```

Published as `Printcraft-android-arm64.apk` on the `android-v1` release (62 391 495 bytes,
`2026-10-24T13:35:48Z`), replacing the pre-sync asset uploaded on 2026-10-19.
