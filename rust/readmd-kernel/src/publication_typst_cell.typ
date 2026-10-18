// Scope emergency wrapping to bibliography cells. Short words retain native
// shaping; overlong ASCII identifiers acquire break opportunities without
// inserting soft hyphens, zero-width characters, or changing copied text.
// Complex scripts keep their native grapheme shaping and line breaking.
#let readmd-csl-cell(body) = layout(size => {
  let available = size.width
  [
    #show regex("[A-Za-z0-9_]+"): it => context {
      // Preserve native shaping inside each fitted rich fragment, but allow a
      // break between adjacent fragments. Separate CSL text/number leaves
      // can each fit while their joined word exceeds the actual cell width.
      if measure(it).width <= available { box(it) }
      else { it.text.clusters().map(cluster => box(text(cluster))).join() }
    }
    #body
  ]
})
