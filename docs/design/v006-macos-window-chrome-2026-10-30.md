# macOS native window controls

The user confirmed that the requested top controls are the red/yellow/green
window buttons. Keep ReadMD's current reader, sidebar and single-line editor.

## Contract

- Use AppKit's actual standard close, miniaturize and zoom/fullscreen buttons.
  Do not replace them with HTML circles or Windows caption icons.
- Extend the existing 48-point application header into a transparent native
  titlebar. The system buttons occupy its physical left edge. Reserve their
  measured bounds plus a small gap; logo, filename and actions cannot overlap.
- The user refined the Mac layout: mirror only this top row. Traffic lights
  stay left, all web header items align right, logo at the far right, filename
  next to its left, then the global actions in reverse physical order. The
  document/editor toolbar below retains its existing order. This physical Mac
  arrangement remains stable in RTL without changing document direction.
- Native buttons keep system hover, focus, accessibility and modifier behavior.
  Window title and light/dark appearance follow the existing application state.
- Dragging empty header space and double-click zoom use the host. Interactive
  controls remain clickable. Other platforms retain their current chrome.
- Zen hides both the web header and ordinary-window traffic lights without
  reserving hidden layout height. Hover/focus reveals the existing header and
  native controls together. Fullscreen lets AppKit own its native reveal panel.
- Native fullscreen entry enables Zen once. Cancelling Zen while fullscreen
  remains possible; exiting fullscreen also leaves Zen, as already specified.
- Close still runs the existing dirty-document/recovery decision. It must not
  become a force-quit shortcut. This slice does not change the product's tray or
  application lifetime policy.

## Verification

Use a real Cocoa/WKWebView integration candidate, production CSP/auth and an
owned synthetic profile. Measure native button bounds, header/document geometry,
1024x680 and 1160x820, dark/light and RTL. Invoke the actual AppKit standard
button actions in the debug-only native driver, checking minimize/restore,
fullscreen/Zen independence and dirty close cancellation. Retest save/reopen and
all export formats against the new binary. CLI/headless CSS alone is insufficient.

Preserve first failures and source/binary/boot hashes. Desktop screenshots, if
available, must contain only the owned app window and synthetic material.

The first mirrored native candidate passed 27 Cocoa/WKWebView checks on this
Mac: 1024x680, 1160x820 and the largest available logical viewport; physical
left system buttons, right-aligned mirrored actions, RTL, dark/light, released
Zen height, actual minimize/restore and fullscreen entry/exit, independent Zen
exit, dirty-close cancellation, save and native edited-indicator clearing.
The owned 1160x820 window capture is 2320x1640 pixels at native 2x scale; it is
not a native 4K viewport result. Final release binaries require their own rerun.
The driver waits for the fullscreen viewport to settle before a second actual
green-button click: AppKit's style flag precedes completion of its Space
animation. The initial premature-click timeouts remain recorded locally.

References: [Apple windows guidance](https://developer.apple.com/design/human-interface-guidelines/windows),
[AppKit transparent titlebar](https://developer.apple.com/documentation/appkit/nswindow/titlebarappearstransparent).
