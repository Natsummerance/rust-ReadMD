# Embedded native presentation designs

These five art seeds are compiled from the user's original ten-slide designs:
合流 (Confluence), 万象光场 (Lightfield), 浮光 (Pearl), 液相 (Chrome),
and 在场 (Field). They are reusable native PresentationML packages, rather
than screenshots of the source slides or palette variants of a common layout.

This directory is compile-time data owned by `readmd-kernel`. The runtime embeds
its seeds, metadata and previews in the executable. It deliberately lives
outside the repository's distributable `assets` tree so release packaging does
not ship the same template bytes a second time. Do not copy it into `assets`.

Each `<style>.pptx` contains ten original art compositions with editable native
shapes, original masters/themes/layouts, transparent artwork, safe art animation
branches and native slide transitions. The source originals are read only.

The compiler removes source text, charts, tables, embedded workbooks, evidence
certificates, the institution's vector logo, document properties,
speaker notes, source navigation, external links, 3D models and embedded fonts.
Art object names are replaced with neutral IDs. Only reachable art dependencies
are included. Chrome's letter artwork and all Field source photographs/mattes
are replaced; neither their old pixels nor their original media parts survive.
Field uses newly imported Point Reyes coastal photography documented in
`tools/pptx-template-photo-sources.json` and fresh foreground land mattes traced
against the exact corresponding photograph crops.

## Runtime contract

`<style>.json` has schema 1 and a public ID `pptx-<style>`. Coordinates are on a
960 × 540 canvas. `widthEmu` and `heightEmu` record the exact native dimensions;
the source width is one EMU smaller than 960 × 12700.

`roles` maps cover, section, content, image, table and closing to source slides.
`contentVariants` adds the other original compositions. A title's `shapeId` is
source provenance: its text shape has already been removed. The runtime inserts
authored editable text, tables and images in the supplied rectangles using IDs
at or above 10000. It must use each frame's text colors, because the original
Confluence and Field designs alternate dark, light and saturated backgrounds.

`foregroundShapeIds` identifies surviving original foreground art; insert
authored content before those shapes to preserve the native depth order.
`imageRect`, where provided, is the source design's open image region.

`previews/<style>-<slide>.png` are art-only stills, generated from the same
sanitized seeds. `previews.rs` is a generated static byte registry. Chrome stills
show one pose. Each render also updates `assets/data/slide-covers/<style>.jpg`
from that style's first preview. Static preview is not animation playback. Chrome uses an original,
continuously deformed closed fluid mesh with polished chromium and studio
reflections, with no font, letter geometry, source mesh or source texture. Its
48 unique cover poses fill all 96 original picture targets in two loops; 48
additional body poses and a new poster replace the other letter media. The
original non-letter blue/black refraction background is retained. The original
picture transforms and timing remain. Font family names are preserved, without bundling
font binaries; the presentation viewer supplies its normal font fallback.

## Rebuilding and verification

Use Python with Pillow and defusedxml (the local PPTX skill environment provides
both), then run:

```sh
blender --background --factory-startup --python tools/render-pptx-fluid.py -- --output <fluid-frames>
python tools/fetch-pptx-photos.py --output <public-domain-photos>
python tools/compile-pptx-templates.py --source <native-style-library> --fluid-frames <fluid-frames> --field-photos <public-domain-photos> --render
```

`--render` uses an installed ibisOffice CLI to render the art-only stills.
`--ibis <executable>` overrides its location, and repeated `--style <id>` options
limit the rebuild. A render/build of ReadMD is not required to compile the data.
No source library, Python, Office host, or rendering CLI is required at runtime.
Chrome/Field rebuilds require the explicit replacement inputs and never silently
restore old letter media or Field photographs. The photo importer checks each NPS rights record before
download. Portrait film-scan borders are cropped before any layout crops; wide
digital photographs are used for the main plates. Source URLs, credits and
import hashes accompany Field metadata without recording local input paths.
`python tools/compile-pptx-templates.py --verify-only` checks the compiled seeds
without accessing the original library or modifying the packages.
The verifier also rejects a duplicate `assets/pptx-templates` directory.

The compiler verifies XML parsing, package relationships, unique native shape
IDs, animation shape/time targets, absence of sample text and private paths,
and exclusion of source notes/data/model/font parts. Each production seed was
also validated against its original with the PPTX skill's `validate.py
--original`, and all fifty art-only slides were visually inspected.

Fluid and Pearl art images are bounded to 800 pixels per axis; new Field photos
may use 1800 pixels to support crisp broad plates. Media is stripped of private
metadata. Fluid PNGs retain their RGB channels with at most 2/255 rounding per
channel and unchanged 8-bit alpha, avoiding palette banding while reducing their
size. Pearl art and Field photographs retain full color. The new Field foreground
mattes follow only the nearest headland/rock with one continuous, antialiased
edge, leaving the distant shore in the plate. `sourceSha256` identifies the original input without
recording local filesystem paths.
