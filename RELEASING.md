# Releasing

The whole procedure for cutting a release. It is kept here, out of `AGENTS.md` and the README, because
those describe the project (which does not change per release) while this describes a moment (which
does). The version lives in the tag and in `$VERSION` below, never in the text here, so a release is a
command run rather than a file edit.

## What a release ships: exactly three assets

| Asset on the release page | Built from | Why it is there |
| --- | --- | --- |
| `Lenovo-Conservation-Scheduler-setup.exe` | `src-tauri/target/release/bundle/nsis/*-setup.exe` | the README's installer link |
| `Lenovo-Conservation-Scheduler-portable.exe` | `src-tauri/target/release/lenovo-conservation-scheduler.exe` | the README's portable link |
| `Lenovo-Conservation-Scheduler.msi` | `src-tauri/target/release/bundle/msi/*.msi` | deploying with a policy (Intune, SCCM, Group Policy) |

Every name is version-less on purpose. The README and the release notes link to
`https://github.com/fajarwz/lenovo-conservation-scheduler/releases/latest/download/<name>`, which
resolves to that filename on the newest release, so those links keep working only while every release
carries those exact names. The bundler bakes the version into its own names
(`Lenovo Conservation Scheduler_1.0.0_x64-setup.exe`), and those would 404 the day 1.0.1 exists.
Use hyphens, not spaces: `gh` rewrites spaces in asset filenames to dots, which breaks any link that
names them.

**Do not upload the bundler's version-named artifacts at all**, not even beside a stable-named copy.
The stable copy is the same bytes, so the versioned name adds nothing but a second way to pick the
wrong file: a page offering `...-1.0.0-portable.exe` beside `...-portable.exe` turns a one-click
download into a guess. One asset per format, one name per asset.

## Cutting v<VERSION>

```bash
VERSION=1.0.1          # the tag is v$VERSION
```

**1. Bump the version** in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, then
let `Cargo.lock` follow:

```bash
cd src-tauri && cargo check && cd ..
```

**2. Frontend, lints and tests.** `--test-threads=1` is not optional: one test writes the real
charging mode and restores it.

```bash
npm run build
cd src-tauri && cargo fmt && cargo clippy --all-targets && cargo test --lib -- --test-threads=1 && cd ..
```

**3. Build the executable and the installers.** Kill a running copy first, or the build fails with
`Access is denied` on the executable.

```bash
taskkill /F /IM lenovo-conservation-scheduler.exe   # if it is running; read its log first
touch src-tauri/src/lib.rs src-tauri/src/main.rs    # cargo does not relink a rebuilt dist/
npm run tauri build
```

Touch the crate script too (`touch src-tauri/build.rs`) if the icons changed: the Windows icon resource
is written by the build script, so a cached resource would keep the previous icon in the exe. `AGENTS.md`
has the full explanation of both traps.

**4. Stage the assets under the names the links use.** Copy, do not move - the bundler's tree is what
the next release builds on.

```bash
rm -rf release-assets && mkdir release-assets
cp src-tauri/target/release/lenovo-conservation-scheduler.exe \
   release-assets/Lenovo-Conservation-Scheduler-portable.exe
cp src-tauri/target/release/bundle/nsis/*-setup.exe \
   release-assets/Lenovo-Conservation-Scheduler-setup.exe
cp src-tauri/target/release/bundle/msi/*.msi \
   release-assets/Lenovo-Conservation-Scheduler.msi
```

**5. Hashes for the notes**, from the staged copies, so the names in the notes are the names on the
page:

```bash
cd release-assets && sha256sum * && cd ..
```

**6. Tag and publish.** Upload exactly what is in `release-assets`: no other asset belongs on the page.

```bash
git tag "v$VERSION" && git push origin "v$VERSION"
gh release create "v$VERSION" release-assets/* \
  --title "Lenovo Conservation Scheduler $VERSION" \
  --notes-file release-notes.md
```

**7. Verify the page and the links.** A release is not done because `gh` exited 0.

```bash
gh release view "v$VERSION" --json assets --jq '.assets[] | "\(.downloadCount)\t\(.size)\t\(.name)"'
for n in Lenovo-Conservation-Scheduler-setup.exe Lenovo-Conservation-Scheduler-portable.exe Lenovo-Conservation-Scheduler.msi; do
  printf '%s -> ' "$n"
  curl -sL --range 0-99 -o /dev/null -w 'http=%{http_code} got=%{size_download}\n' \
    "https://github.com/fajarwz/lenovo-conservation-scheduler/releases/latest/download/$n"
done
```

Both must answer `http=206` with a size of 100. `http=404` means an asset name is wrong, or the release
is still a draft.

## The release notes

Prose plus hashes. Keep the description in the README and let the notes link to it: the same feature
described in two places is how a release page ends up advertising a download that no longer exists.

~~~markdown
# Lenovo Conservation Scheduler <VERSION>

<one or two sentences: what this release is. For a first release, what the app does>

## What's new

- <the change, in the terms a user feels it in>

## Fixed

- <what was broken, what it did, what it does now>

## Install

| Download | Size | What it is |
| --- | --- | --- |
| [Installer](https://github.com/fajarwz/lenovo-conservation-scheduler/releases/latest/download/Lenovo-Conservation-Scheduler-setup.exe) | <size> MB | Installs per user, needs no administrator rights, and adds the "start with Windows" option. |
| [Portable exe](https://github.com/fajarwz/lenovo-conservation-scheduler/releases/latest/download/Lenovo-Conservation-Scheduler-portable.exe) | <size> MB | Nothing to install and nothing beside it; do not move it after the first run, because the "start with Windows" entry records the path it was launched from. |
| [MSI](https://github.com/fajarwz/lenovo-conservation-scheduler/releases/latest/download/Lenovo-Conservation-Scheduler.msi) | <size> MB | For deploying with a policy - Intune, SCCM, Group Policy. |

All three links are deliberately version-less, so they resolve to the newest release and keep working
after this one.

> [!WARNING]
> These builds are **unsigned**, so SmartScreen shows *"Windows protected your PC"* on the first run:
> click **More info** -> **Run anyway**, or unblock the file first (right-click -> Properties ->
> **Unblock**). A code-signing certificate is what removes the dialog; there is none in <VERSION>.

## Checking your download

SHA-256 of each file above, from step 5:

```text
<paste the sha256sum output>
```

## What it does, and what it costs

The [README](https://github.com/fajarwz/lenovo-conservation-scheduler#readme) describes the current
version, so it is not repeated here.
~~~

## If an asset does not belong on the page

Someone will eventually upload the whole `bundle/` tree. Delete the extras and correct the notes in
the same breath - the notes name every asset and quote its hash, so a deleted asset leaves the page
describing a file that is not there.

```bash
gh api repos/fajarwz/lenovo-conservation-scheduler/releases/tags/<tag> \
  --jq '.assets[] | "\(.id)\t\(.download_count)\t\(.name)"'
gh api -X DELETE repos/fajarwz/lenovo-conservation-scheduler/releases/assets/<id>
gh release edit <tag> --notes-file <corrected.md>
```

Nothing is lost: a version-named copy's bytes and its SHA-256 are the stable-named twin's, which stays.

## If this file starts needing a per-release edit

That is the signal that the release needs fewer moving parts, not a longer document. Every version
number or asset name written down here is one that has to be found and changed again next time.
