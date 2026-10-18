# ReadMD's locked Hayagriva patch

Base: Hayagriva 0.10.1, upstream Git revision
`e51299ecd1a9f62b2f069af2f554ae4e84d50527`.
The dependency source is copied from the existing Cargo registry package;
186 original files, 3,772,254 bytes, excluding upstream tests and CLI assets.
No dependencies, archive styles or locale data are added or changed.

The original MIT / Apache-2.0 licenses and NOTICE are retained. Archive styles
and locales retain their existing CC BY-SA 3.0 notices and attribution. The
ReadMD changes to source code use the same MIT / Apache-2.0 license choice.

Modified source files:

- `src/csl/mod.rs`: bounded, structured per-item prefix/suffix; a public
  `SuppressAuthor` purpose using the existing special form; author-only legal
  substitutes from the same CSL tree; separate author/remainder elements for
  prose citations in note styles; real note-number distances for near-note and
  ibid state in both speculative and final passes, including the preceding
  single-cite requirement; author-only isolation from subsequent/first-note state; numeric
  and year collapse boundaries that retain per-item explanations and locators.
- `src/csl/elem.rs`: two metadata variants identifying a note-style prose
  citation's inline author and note remainder. Existing formatted element
  representation remains the transport. Processor terms and the first
  rendered date year retain typed identity for note capitalization and ranges.
- `src/csl/rendering/mod.rs`: identify processor-owned terms and the first
  rendered year without rewriting authored text or losing formatting.
- `src/csl/rendering/names.rs`: select the original primary names node during
  actual rendering, including legal substitutes, for suppress-author output.

Original file SHA-256 values:

| File | SHA-256 |
| --- | --- |
| `src/csl/mod.rs` | `6b7d076ea3ca13ada9585d9e790c5243a4cbfa8267d5b8dea049a10e4114f5b0` |
| `src/csl/elem.rs` | `4262a4507ab6a3f1c9f177ffedac5ac005ef20fb2257d9b97f39b390b7df3d4f` |

Independent processor RESULT fixtures remain private QA evidence. Their
license does not permit assuming redistribution rights; no such fixture,
golden text or alternative processor source is included in this vendor tree.
ReadMD's regression cases are independently authored in its kernel modules.
