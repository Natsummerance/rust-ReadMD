# Website motion and offline verification

The existing Apple layout, typography, themes and genuine recordings remain the source of the site. `tools/build-motion-pages.mjs` adds the shared motion assets to all 50 HTML pages and generates six localized scroll chapters on the four homepages. Components reveal on entry, cards respond to the pointer, controls respond to hover/press/focus, and the sticky product window follows scroll position in both directions.

`public/assets/motion.js` uses native browser observers and animation frames. At most two video elements are kept, seeking is coalesced, and offscreen work stops when it settles. Background tabs pause motion. Reduced motion exposes every chapter without requesting video; disabling JavaScript leaves readable content and working links. Videos stay muted and paused: scroll position selects the displayed frame. No brightness filter covers the software interface.

Build with the already prepared dependency tree:

```sh
npm run verify:release --prefix website
```

The build does not install packages or download assets. If FFmpeg is already on PATH, `tools/build-scroll-media.mjs` creates six VP9 derivatives in ignored `dist/motion-clips/`. Original MP4s remain unchanged. Source and output hashes permit reuse. Without FFmpeg the existing MP4 sources and poster fallback remain usable; do not add an installation step to the build. Media derivatives never enter the software commit.

The six historical recordings are natively 1280×800; converting their codec does not increase their detail. New V0.0.5 incremental recordings are natively 2560×1600 and remain in the local production directory. Do not label the historical footage as 2K or 4K.

Run actual browser verification with the existing Playwright installation:

```sh
NODE_PATH=../ui-tests/node_modules READMD_MOTION_CHANNEL=chrome node tools/motion-smoke.cjs
```

On Windows, set those environment variables in PowerShell and use `msedge` for the installed Edge. `READMD_MOTION_WEBKIT=1` adds WebKit on hosts with a working VP9 decoder; the prepared Windows WebKit could not decode these clips, so its video test is not marked as passing. This does not verify macOS Safari. `READMD_MOTION_TEST_MP4=1` also aborts WebM requests to verify actual MP4 fallback in branded Chrome/Edge. `READMD_WEBSITE_MOTION_REPORT` optionally writes evidence and screenshots. Tests cover four languages, six chapters, reverse and exact-boundary seeking, narrow layouts, dark mode, repeated scrolling, failed media, reduced motion and JavaScript disabled. CI uses the prepared Chrome browser; codec support differs between bundled Chromium and branded browsers.

Public version/download metadata continues to use `release.json.stable` until the candidate is officially published. Deploying this site is a separate step from building or verifying it.
