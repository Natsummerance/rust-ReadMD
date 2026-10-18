// Ordinary-body typed spacing only. Measurements contain text and style,
// never counters, footnotes, author code, anchors, images or resource writes.
#let readmd-typed-paragraph(body, metric: [], mode: "multiple", requested: 11pt,
    base-size: 11pt, object-top: 0pt, object-bottom: 0pt, after: 0pt,
    marker: "", block-index: 1, block-id: none, body-height: 0pt) = context layout(size => {
  let upper = calc.max(0.8 * base-size,
    measure(box(text(top-edge: "bounds", bottom-edge: "baseline", metric))).height)
  let lower = calc.max(0.2 * base-size,
    measure(box(text(top-edge: "baseline", bottom-edge: "bounds", metric))).height)
  if mode == "exact" {
    upper = calc.max(upper, object-top)
    lower = calc.max(lower, object-bottom)
    if upper + lower > requested + 0.000001pt {
      let details = (coordinateSpace: "pdfParagraphLine", blockIndex: block-index,
        line: 1, mode: mode, requestedPt: requested / 1pt,
        requiredPt: (upper + lower) / 1pt, availableWidthPt: size.width / 1pt,
        availableHeightPt: body-height / 1pt)
      if block-id != none { details.insert("blockId", block-id) }
      panic(marker + json.encode(details, pretty: false))
    }
  }
  // Native inline SVGs may enlarge individual Multiple/AtLeast lines. Exact
  // uses the conservative paragraph union, keeping those objects unchanged.
  let leading = calc.max(0pt, requested - upper - lower)
  [
    #set text(top-edge: upper, bottom-edge: -lower)
    #set par(leading: leading, spacing: leading + after)
    #par(body)
  ]
})
