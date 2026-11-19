// Fit CSS-pixel preferences (already converted to pt) in the actual Typst
// column, including a narrower list, quote, or local publication block.
#let readmd-table-columns(widths, available) = {
  let fixed = widths.filter(width => width != none).sorted()
  if fixed.len() == 0 {
    return widths.map(_ => available / widths.len())
  }
  let flexible = widths.len() - fixed.len()
  let floor = calc.min(24pt, available / widths.len())
  let budget = available - flexible * floor
  let desired = fixed.sum()
  let scale = calc.min(1, budget / desired)
  if scale < 1 {
    for width in fixed.slice(0, fixed.len() - 1) {
      if width * scale >= floor { break }
      budget -= floor
      desired -= width
      scale = budget / desired
    }
  }
  let columns = widths.map(width => if width == none { 0pt } else { calc.max(floor, width * scale) })
  if flexible > 0 {
    let rest = calc.max(floor, (available - columns.sum()) / flexible)
    columns = widths.zip(columns).map(((width, actual)) => if width == none { rest } else { actual })
  }
  columns
}

#let readmd-table-tracks(widths, minimums, groups, heads, body, inset, failure, draw) = layout(size => {
  if size.width <= 0pt { panic(failure) }
  let columns = readmd-table-columns(widths, size.width)
  let rows = minimums.map(_ => auto)
  for cell in heads + body {
    if columns.slice(cell.x, cell.x + cell.colspan).sum() <= 2 * inset { panic(failure) }
  }
  if groups.len() > 0 {
    // Native fixed rows are unbreakable. Assign them only to the header and
    // body groups affected by a minimum, using native layout for text height.
    // Unsized and naturally oversized unmerged rows keep native pagination.
    let heights = minimums.map(height => if height == none { 2 * inset } else { calc.max(height, 2 * inset) })
    let measured = minimums.map(_ => false)
    measured.at(0) = true
    for (first, end) in groups {
      for index in range(first, end) { measured.at(index) = true }
    }
    let spans = ()
    for cell in heads + body {
      let start = cell.y
      if not measured.at(start) { continue }
      let width = columns.slice(cell.x, cell.x + cell.colspan).sum() - 2 * inset
      let required = measure(align(cell.align, cell.body), width: width).height + 2 * inset
      if cell.rowspan == 1 {
        heights.at(start) = calc.max(heights.at(start), required)
      } else {
        spans.push((start: start, end: start + cell.rowspan, height: required))
      }
    }
    for span in spans.sorted(key: span => span.end) {
      let actual = heights.slice(span.start, span.end).sum()
      heights.at(span.end - 1) += calc.max(0pt, span.height - actual)
    }
    let capacity = size.height - heights.first()
    if capacity < 0pt or (body.len() > 0 and capacity <= 2 * inset) { panic(failure) }
    if minimums.first() != none { rows.at(0) = heights.first() }
    for (first, end) in groups {
      if first == 0 { continue }
      let planned = heights.slice(first, end).sum()
      if planned > capacity {
        if end > first + 1 or minimums.at(first) > capacity { panic(failure) }
        // The authored minimum fits, and the text itself is taller than a
        // fresh page. Native pagination can still split the row's content.
        continue
      }
      for index in range(first, end) { rows.at(index) = heights.at(index) }
    }
  }
  draw(columns, rows, heads, body)
})
