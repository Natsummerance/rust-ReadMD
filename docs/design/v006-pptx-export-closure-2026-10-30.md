# V0.0.6 PPTX export correction

The export dialog's slide tab must save an actual editable `.pptx`. The existing
Reveal.js full-screen presentation and its explicit HTML API remain available.
An HTML document with a PowerPoint label or extension is not a valid result.

## First complete slice

- Input is the same revision-bound export snapshot, frozen image bytes, options
  and review consent used by the other writers. Preview never accepts edits.
- A Rust-owned 16:9 deck layout feeds both the preview and PresentationML writer.
  Text, emphasis, lists, links, code and tables are native editable slide objects.
  Images retain their proportions and actual frozen bytes. Speaker notes survive.
- Explicit slide separators and headings establish boundaries. Excess body,
  code and table rows continue on additional slides instead of clipping or
  shrinking everything to unreadable type. Table headers repeat on continuation.
- Unsupported content must produce a visible diagnostic or fail before output;
  it must never disappear silently. Mathematical/diagram image projections must
  identify their editability boundary and retain their source in notes.
- The slide thumbnail fills its available width at 16:9. Enlarged preview fits
  both viewport dimensions, changes size with the window and has working page
  navigation. No A4 paper padding, fixed miniature scaling or old-format residue.
- Readiness includes the current snapshot, actual slide content, loaded fonts
  and decoded images. Late, failed and cancelled results retain existing guards.
- ZIP, XML, Markdown, image decoding and resource safety reuse locked existing
  components. Generating PPTX does not require Office, Python or a host converter.

## Independent verification before certification

1. Open, preview, export with real authorization, reopen and inspect the ZIP's
   content types, relationships, editable text/table objects and media bytes.
2. Render the generated PPTX through an independent office application, inspect
   every page and compare slide count, content and layout to the shared preview.
   Structural validation alone is not external-application acceptance.
3. Cover Chinese text, bold/list/code/table/image/link/notes, overflowing content,
   missing resources, cancelled export, locked output and review consent.
4. Check preview geometry at 1024x680, 1160x820 and 4K/high-DPI, including resizing
   and switching back to PDF/DOCX. Re-record footage only after actual readiness.

Advanced PowerPoint authoring, arbitrary imported slide-layout fidelity and a
complete Office/WPS/Keynote matrix are separate V0.0.7 work. They do not become
accepted merely because this bounded Markdown-to-PPTX flow passes.
