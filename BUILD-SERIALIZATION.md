# Team build serialization (disk constraint)

Free space on this machine is ~14 GB. One app's build footprint:

| Artifact | Size |
|---|---|
| `<app>/target` (Rust) | ~2.0 GB |
| `<app>/apps/<app>-android/android/app/build` (Gradle) | ~0.5 GB |
| `apps/<app>-android/jniLibs/arm64-v8a/libmain.so` | ~45 MB |

Four concurrent builds = ~10 GB and will exhaust the disk mid-build. **Therefore: develop in
parallel, build serially.**

Take this lock around every `cargo ndk ... build --release` and every `gradle assembleRelease`:

```bash
/tmp/artcraft-build-lock.sh bash -c 'cd ~/artcraft-mobile/<app> && cargo +stable ndk -t arm64-v8a \
  -o apps/<app>-android/jniLibs build --release -p <app>-android'
```

`flock` blocks until the slot is free, so it is safe to just take it and wait. Do not run two
heavy builds at once, and do not work around the lock.

---

## Disk hygiene — REQUIRED after every successful APK

The machine is at 98% capacity. Freed so far: NDK 26 (-3.0G), probe target (-0.9G), homebrew +
npm caches (-1.2G), filmcraft target (-2.0G), filmcraft gradle build (-0.5G), gradle zip (-0.1G).
Free space is now ~21 GB. Do not give it back.

Once your APK is verified and copied to `dist/<App>-<version>-android-arm64.apk`, delete your own
build trees — the APK in `dist/` is the deliverable, not the target directory:

```bash
rm -rf ~/artcraft-mobile/<app>/target
rm -rf ~/artcraft-mobile/<app>/apps/<app>-android/android/app/build
```

Never delete:
- `dist/` (published APKs) or `shots/` (evidence)
- `~/Library/Android/sdk/ndk/28.2.13676358` (the toolchain)
- `~/artcraft-mobile/tools/gradle-9.3.1/` (manual Gradle — the wrapper cannot download)
- `~/.gradle/caches` (Gradle deps are proxy-hostile to re-fetch)
- `~/.android/avd/` (the emulator)

If a build fails with `No space left on device`, run `df -h /System/Volumes/Data` and report to the
Lead rather than deleting another teammate's in-flight build.
