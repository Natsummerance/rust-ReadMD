ReadMD capability component notices

capability-components.json records pinned versions, loading decisions and the
scope of the current review. Cargo.lock and the collaboration package-lock.json
are authoritative for exact transitive versions. Builds run offline.

Hayagriva Rust source is used under its MIT option. Its bundled CSL styles and
locales have a separate CC-BY-SA-3.0 license; they are not relicensed as MIT.
See hayagriva-NOTICE.txt, CC-BY-SA-3.0.txt and csl-attribution.json for the
unmodified archive's per-style original identifiers, authors and contributors.
The attribution is generated offline from Hayagriva 0.10.1, not a newer remote
copy. Run the readmd-kernel example dependency_notices to regenerate it.

Yjs and lib0 notices are in assets/vendor and accompany the lazy collaboration
bundle. esbuild is a development tool and is not shipped as an application
runtime. Other existing vendored libraries retain their own adjacent licenses.

These files must be retained when packaging the assets directory. A dependency
review or passing XML test does not certify complete mathematical, publication
or platform compatibility; those states are tracked by the acceptance ledger.
