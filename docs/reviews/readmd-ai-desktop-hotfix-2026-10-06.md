# V0.0.4 AI model discovery and desktop extension hotfix

The shipped `/api/ai/models` route read the retired `aiConfig` settings blob,
ignoring the provider, base URL and credential handle posted by the current UI.
It returned an empty cached list with HTTP 200 even when no upstream request ran.

Discovery now resolves the active `ai.json` connection and OS/encrypted-vault
credential through the shared provider directory. Explicit credential handles
must belong to the selected provider. OpenAI-compatible bearer authentication,
Anthropic authentication/version headers, custom non-secret headers and keyless
local discovery are covered by loopback HTTP tests. Invalid/HTML responses try
the next discovery endpoint; authentication failures stop immediately. Errors
do not expose upstream bodies or credentials, and do not masquerade as success.
The UI persists discovered models and the selected model, including newly
entered replacement keys.

Desktop runtime files, bridge state and WebView cache now belong under
`data_dir/plugins/pet`, rather than the software installation directory.
Protected Program Files, macOS bundles and system directories therefore do not
need to be writable. Existing bundled archives remain the installation source.
Native startup restores the saved runtime choice. Failed installation/launch
keeps the actual failure state instead of silently reporting a different runtime
as enabled. An installation completed from the settings panel reapplies the
current settings.

Ordinary checkboxes use a 14px theme-aware outline/tick and visible keyboard
focus. Existing switch tracks retain their behavior. The AI composer uses one
input surface, wrapping controls and a 44px label/button click target.

Verified locally:

- The saved Antigravity connection returned 23 real upstream models with the
  new kernel. No key was displayed or logged. The upstream did not advertise
  `gemini-3.8-flash-high`; discovery reports the service's actual list.
- Five model-discovery HTTP regression tests and 58 pet module tests passed.
- Six desktop UI tests passed, including actual bundled Rust overlay installation
  in a temporary user data directory, launch, ready health report and shutdown.
  A further failed-install UI regression passed.
- Fifteen mobile interaction/layout tests passed; the native overlay case was
  deliberately skipped on the mobile project.
- Composer controls were checked at 1160×820, 1024×680 and 390×844 across themes.
  Dark/light 2× screenshots were visually reviewed for tick shape and layout.
- Nine release-publisher/packaging tests and assets, wiring, i18n and style
  gates passed. Headless and native Windows kernels were built offline/locked.
- Complete headless Rust library regression: 1854 passed, zero failed,
  three environment-dependent tests ignored.

`ui-tests/ai-model-discovery.spec.js` now runs in frontend CI. The native overlay
case additionally requires `READMD_VERIFY_NATIVE_PET=1` and a real bundled ZIP;
browser-only CI explicitly skips it.
