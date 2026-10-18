// Limited continuous-section balancing for the pinned Typst 0.15.1 engine.
// The writer supplies already rendered native flow blocks in original order.
// A heading and every keep-with-next successor must be one indivisible unit.
// `safe` must be false for notes, floats, explicit breaks, or layout-sensitive
// state/query content. Other column counts and unsafe/large flows retain the
// native columns path, including its currently unbalanced last-page behavior.
// No content is split, reconstructed, scaled, clipped, or assigned a fixed
// total height across pages. In particular, measure(columns, height:H) is NOT
// an overflow predicate: the pinned engine can return H while content spills.
// The caller must surface a limitation when diagnostic.balanced is false.
// Page promotion near the bottom remains a native-flow limitation, even when
// a later layout pass finds room to balance on the following page.
#let readmd-section-columns(
  parts, width, bottom,
  count: 2, gutter: 8mm, after: 0pt, safe: true, id: "",
) = context {
  let start = here().position()
  let available = bottom - start.y
  let native = columns(count, gutter: gutter, parts.join())
  let best = none
  let measurements = 0
  if safe and count == 2 and parts.len() > 1 and parts.len() <= 512 and available > 0pt {
    let low = 1
    let high = parts.len() - 1
    let column-width = (width - gutter) / count
    // Positive native-flow units make prefix height nondecreasing and suffix
    // height nonincreasing. Ten bounded cuts cover at most 512 grouped blocks;
    // each cut performs two whole-flow measurements, O(n log n) layout work.
    for iteration in range(10) {
      if low > high { break }
      let cut = calc.floor((low + high) / 2)
      let left = parts.slice(0, cut).join()
      let right = parts.slice(cut).join()
      let left-height = measure(left, width: column-width).height
      let right-height = measure(right, width: column-width).height
      measurements += 2
      let height = calc.max(left-height, right-height)
      if best == none or height < best.height {
        best = (cut: cut, height: height, left: left, right: right)
      }
      if left-height < right-height { low = cut + 1 } else { high = cut - 1 }
    }
  }
  let fits = best != none and best.height <= available
  let reason = if fits { "balanced" }
    else if not safe { "context-sensitive-flow" }
    else if count != 2 { "column-count" }
    else if parts.len() > 512 { "block-budget" }
    else if parts.len() <= 1 { "indivisible-flow" }
    else { "remaining-height" }
  [#metadata((
    id: id, balanced: fits, reason: reason, count: count,
    blocks: parts.len(), measurements: measurements, start: start,
    remaining: available, height: if best == none { none } else { best.height },
  ))<readmd-section-columns-diagnostic>]
  if fits {
    block(height: best.height, breakable: false, below: after)[
      #columns(count, gutter: gutter)[#best.left#colbreak()#best.right]
    ]
  } else { native }
}
