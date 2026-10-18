# Offline desktop pet build and verification

The Windows host captures Raw Input in Rust and delegates dragging to user32.
Only BongoCat uses the original BongoCat standard keyboard/mouse model;
Mochi and the other sprite characters keep their ReadMD artwork. The presentation
uses the existing Tao/WRY and PIXI/Cubism renderer. It is not a new Rust GPU
renderer. Upstream attribution is in `third_party/bongocat/`.

Node 22.13+ can rebuild the tracked TypeScript and JavaScript without npm/Vite
installation. Supply a previous ReadMD renderer as the library/character cache:

```powershell
node packages/readmd-pet-rust/scripts/build-renderer.mjs --renderer-cache <previous-runtime>/renderer --output packages/readmd-hermes-pet-adapter/dist/renderer
$env:CARGO_NET_OFFLINE = 'true'
cargo build --offline --release --manifest-path packages/readmd-pet-rust/Cargo.toml
cargo run --offline --manifest-path rust/Cargo.toml -p xtask -- pet-package --skip-build --output <new-package-directory>
```

Keep cache and output directories different. The renderer build reads the
four pinned PIXI/Cubism chunks and any existing sprites from the cache;
ReadMD entry, stage, companion layer and BongoCat model come from tracked source.
No network, dependencies or directory deletion are needed. `CARGO_TARGET_DIR`
is honored by the native packager. `pet-package` rebuilds the presentation again
in the new package directory, preventing a stale dist entry from being shipped.

```powershell
cargo test --offline --manifest-path packages/readmd-pet-rust/Cargo.toml --lib
$env:READMD_PET_RENDERER_BUILD = '<absolute-renderer-build>'
# Use the project's already installed Playwright executable; do not install.
playwright test pet-bongo.spec.js --config ui-tests/playwright.config.js --project=desktop --retries=0
pwsh -NoProfile -File packages/readmd-pet-rust/scripts/smoke-windows.ps1 -Executable <absolute-host-exe> -RendererRoot <absolute-renderer-build>
```

For browser and native tests, prepare a complete fixture in a new cache
directory, rather than pointing tests at the script-only renderer build:

```powershell
node ui-tests/prepare-pet-fixture.cjs --output .cache/pet-verification
$env:READMD_PET_RENDERER_BUILD = (Resolve-Path .cache/pet-verification/renderer).Path
```

This stages all six sprites, the Arch Chan and BongoCat models, pinned library
chunks and the locally available, checksum-verified Cubism Core. Set
`READMD_PET_CUBISM_CORE` if that licensed offline runtime is stored elsewhere.
Missing artwork or runtime files must fail verification rather than count as
a successful extension build. The final `xtask pet-package` is still required
for installer and release-bundle checks.

The native smoke starts an isolated host with temporary bridge/state files. It
checks the drag threshold, actual movement, durable final position, stale
snapshot protection, acknowledgement and transparent-margin click-through.
It restores the pointer, closes only its own host and leaves its fixture for
inspection. It does not read or alter the user's settings or desktop pet data.

The browser checks cover modifier chords, fast press/release edges, independent
mouse buttons, stale input rejection, real Cubism hand/key layers, transparency,
the six characters at 320x420 and 480x560, and the optional Arch Chan stage.
