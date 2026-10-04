//! Dependency-free SVG rendering for the documented basic WSD, D2 and ASCII
//! diagram grammars. Unsupported syntax fails explicitly instead of being
//! dropped or uploaded to an external renderer.
use crate::diagrams::{DiagramError, DiagramResult};
use regex::Regex;
use std::collections::BTreeMap;

fn invalid() -> DiagramError { DiagramError::Expected("diagram_render_failed".into()) }
fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }
fn unquote(s: &str) -> String { s.trim().trim_matches('"').replace("\\n", "\n") }
fn start(width: usize, height: usize) -> String {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\"><defs><marker id=\"native-end\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#3b5bdb\"/></marker></defs><rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>")
}
fn label(out: &mut String, x: usize, y: usize, text: &str, anchor: &str) {
    for (row, line) in text.lines().enumerate().take(12) {
        out.push_str(&format!("<text x=\"{x}\" y=\"{}\" text-anchor=\"{anchor}\" font-family=\"sans-serif\" font-size=\"14\" fill=\"#18191c\">{}</text>", y + row * 18, esc(line)));
    }
}
fn node(nodes: &mut Vec<(String, String)>, key: &str) -> DiagramResult<usize> {
    let key = unquote(key);
    if key.is_empty() || key.len() > 256 { return Err(invalid()); }
    if let Some(index) = nodes.iter().position(|n| n.0 == key) { return Ok(index); }
    if nodes.len() >= 100 { return Err(invalid()); }
    nodes.push((key.clone(), key)); Ok(nodes.len() - 1)
}
pub fn render(engine: &str, code: &str) -> DiagramResult<String> {
    if code.len() > 2 * 1024 * 1024 || code.lines().count() > 2000 || code.trim().is_empty() { return Err(invalid()); }
    match engine { "wsd" => sequence(code), "d2" => graph(code), "ditaa" => ascii(code), _ => Err(invalid()) }
}

fn sequence(code: &str) -> DiagramResult<String> {
    let signal = Regex::new(r#"^\s*("[^"]+"|[^:<]+?)\s*(-{1,2}>{1,2}|<{1,2}-{1,2})\s*("[^"]+"|[^:]+?)\s*:\s*(.*)$"#).unwrap();
    let participant = Regex::new(r#"^participant\s+(?:"([^"]+)"\s+as\s+(\S+)|(.+))$"#).unwrap();
    let note = Regex::new(r"^note (?:over|left of|right of) ([^:]+):\s*(.*)$").unwrap();
    let mut nodes = Vec::new(); let mut events = Vec::new(); let mut title = String::new();
    for line in code.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        if let Some(t) = line.strip_prefix("title ") { title = t.into(); }
        else if let Some(c) = participant.captures(line) {
            let key = c.get(2).or_else(|| c.get(3)).unwrap().as_str(); let index = node(&mut nodes, key)?;
            if let Some(name) = c.get(1) { nodes[index].1 = unquote(name.as_str()); }
        } else if let Some(c) = signal.captures(line) {
            let mut a = node(&mut nodes, &c[1])?; let mut b = node(&mut nodes, &c[3])?;
            if c[2].starts_with('<') { std::mem::swap(&mut a, &mut b); }
            events.push((a, b, c[4].to_string(), c[2].contains("--"), false));
        } else if let Some(c) = note.captures(line) {
            let keys: Vec<_> = c[1].split(',').collect();
            let a = node(&mut nodes, keys[0])?; let b = node(&mut nodes, keys.last().unwrap())?;
            events.push((a, b, c[2].to_string(), false, true));
        } else { return Err(invalid()); }
    }
    if nodes.is_empty() || events.len() > 1000 { return Err(invalid()); }
    let width = (nodes.len() * 190 + 100).max(320); let height = 150 + events.len() * 65;
    let mut out = start(width, height); label(&mut out, width / 2, 24, &title, "middle");
    for (i, (_, name)) in nodes.iter().enumerate() {
        let x = 100 + i * 190;
        out.push_str(&format!("<rect x=\"{}\" y=\"45\" width=\"150\" height=\"50\" rx=\"8\" fill=\"#e9edfb\" stroke=\"#3b5bdb\"/><path d=\"M {x} 95 V {}\" stroke=\"#8e9199\" stroke-dasharray=\"5 5\"/>", x - 75, height - 20));
        label(&mut out, x, 70, name, "middle");
    }
    for (i, (a, b, text, dashed, note)) in events.iter().enumerate() {
        let x1 = 100 + a * 190; let x2 = 100 + b * 190; let y = 130 + i * 65;
        if *note {
            let left = x1.min(x2).saturating_sub(75); let w = x1.abs_diff(x2) + 150;
            out.push_str(&format!("<rect x=\"{left}\" y=\"{}\" width=\"{w}\" height=\"42\" rx=\"4\" fill=\"#fff4c7\" stroke=\"#8a5a00\"/>", y - 25));
            label(&mut out, (x1 + x2) / 2, y, text, "middle");
        } else {
            let path = if a == b { format!("M {x1} {y} h 65 v 24 h -65") } else { format!("M {x1} {y} H {x2}") };
            out.push_str(&format!("<path d=\"{path}\" fill=\"none\" stroke=\"#3b5bdb\" stroke-width=\"2\" marker-end=\"url(#native-end)\"{}/>", if *dashed { " stroke-dasharray=\"5 4\"" } else { "" }));
            label(&mut out, (x1 + x2) / 2, y - 9, text, "middle");
        }
    }
    out.push_str("</svg>"); Ok(out)
}

fn graph(code: &str) -> DiagramResult<String> {
    let arrows = Regex::new(r"\s*(<->|->|<-|--)\s*").unwrap();
    let key = Regex::new(r#"^(?:[\p{L}_][\p{L}\p{N}_. -]*|"[^"]+")$"#).unwrap();
    let mut nodes = Vec::new(); let mut edges = Vec::new(); let mut shapes = BTreeMap::new(); let mut direction = "down";
    for line in code.lines().flat_map(|l| l.split(';')).map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        if line.contains(['{', '}', '[', ']']) { return Err(invalid()); }
        if let Some(d) = line.strip_prefix("direction:") {
            direction = d.trim(); if !matches!(direction, "right" | "left" | "down" | "up") { return Err(invalid()); } continue;
        }
        let (keys, text) = line.split_once(':').unwrap_or((line, ""));
        if arrows.is_match(keys) {
            let parts: Vec<_> = arrows.split(keys).collect(); let ops: Vec<_> = arrows.find_iter(keys).collect();
            if parts.iter().any(|p| !key.is_match(p.trim())) { return Err(invalid()); }
            for (pair, op) in parts.windows(2).zip(ops) {
                let a = node(&mut nodes, pair[0])?; let b = node(&mut nodes, pair[1])?;
                edges.push((a, b, op.as_str().trim().to_string(), unquote(text)));
            }
        } else if let Some(owner) = keys.trim().strip_suffix(".shape") {
            let i = node(&mut nodes, owner)?; let shape = text.trim();
            if !matches!(shape, "rectangle" | "square" | "circle" | "oval" | "diamond") { return Err(invalid()); }
            shapes.insert(i, shape.to_string());
        } else {
            if !key.is_match(keys.trim()) { return Err(invalid()); }
            let i = node(&mut nodes, keys)?; if !text.trim().is_empty() { nodes[i].1 = unquote(text); }
        }
    }
    if nodes.is_empty() || edges.len() > 1000 { return Err(invalid()); }
    // Stable levels for DAGs; cycles retain a finite row rather than iterating
    // indefinitely. All edges are drawn, including back edges and self loops.
    let mut degree = vec![0usize; nodes.len()]; let mut levels = vec![0usize; nodes.len()];
    for (a, b, arrow, _) in &edges { if a != b { degree[if arrow == "<-" { *a } else { *b }] += 1; } }
    let mut queue: std::collections::VecDeque<_> = degree.iter().enumerate().filter(|(_, d)| **d == 0).map(|(i, _)| i).collect();
    while let Some(a) = queue.pop_front() {
        for (from, to, arrow, _) in &edges {
            let (from, to) = if arrow == "<-" { (*to, *from) } else { (*from, *to) };
            if from != a || from == to { continue; }
            levels[to] = levels[to].max(levels[from] + 1); degree[to] = degree[to].saturating_sub(1);
            if degree[to] == 0 { queue.push_back(to); }
        }
    }
    let max_level = *levels.iter().max().unwrap_or(&0); let mut lanes = BTreeMap::<usize, usize>::new();
    let horizontal = matches!(direction, "right" | "left"); let reversed = matches!(direction, "left" | "up");
    let positions: Vec<_> = levels.iter().map(|level| {
        let lane = lanes.entry(*level).or_default(); let at = *lane; *lane += 1;
        let level = if reversed { max_level - level } else { *level };
        if horizontal { (110 + level * 240, 70 + at * 130) } else { (110 + at * 240, 70 + level * 130) }
    }).collect();
    let width = positions.iter().map(|p| p.0).max().unwrap() + 110; let height = positions.iter().map(|p| p.1).max().unwrap() + 90;
    let mut out = start(width, height);
    for (a, b, op, text) in &edges {
        let (ax, ay) = positions[*a]; let (bx, by) = positions[*b];
        let (x1, y1, x2, y2) = if horizontal { (if bx >= ax { ax + 80 } else { ax - 80 }, ay, if bx >= ax { bx - 80 } else { bx + 80 }, by) }
            else { (ax, if by >= ay { ay + 30 } else { ay - 30 }, bx, if by >= ay { by - 30 } else { by + 30 }) };
        let path = if a == b { format!("M {ax} {} c 110 -100 110 100 0 60", ay - 30) }
            else { format!("M {x1} {y1} L {x2} {y2}") };
        out.push_str(&format!("<path d=\"{path}\" fill=\"none\" stroke=\"#3b5bdb\" stroke-width=\"2\"{}{}/>", if op.contains('<') { " marker-start=\"url(#native-end)\"" } else { "" }, if op.contains('>') { " marker-end=\"url(#native-end)\"" } else { "" }));
        label(&mut out, (x1 + x2) / 2, (y1 + y2) / 2 - 8, text, "middle");
    }
    for (i, (_, text)) in nodes.iter().enumerate() {
        let (x, y) = positions[i]; let shape = shapes.get(&i).map(String::as_str).unwrap_or("rectangle");
        let geometry = match shape {
            "circle" => format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"40\""),
            "square" => format!("<rect x=\"{}\" y=\"{}\" width=\"80\" height=\"80\" rx=\"4\"", x - 40, y - 40),
            "oval" => format!("<ellipse cx=\"{x}\" cy=\"{y}\" rx=\"80\" ry=\"30\""),
            "diamond" => format!("<polygon points=\"{x},{} {},{y} {x},{} {},{y}\"", y - 40, x + 90, y + 40, x - 90),
            _ => format!("<rect x=\"{}\" y=\"{}\" width=\"160\" height=\"60\" rx=\"8\"", x - 80, y - 30),
        };
        out.push_str(&format!("{geometry} fill=\"#e9edfb\" stroke=\"#3b5bdb\"/>")); label(&mut out, x, y + 5, text, "middle");
    }
    out.push_str("</svg>"); Ok(out)
}

fn ascii(code: &str) -> DiagramResult<String> {
    let lines: Vec<Vec<char>> = code.lines().map(|l| l.chars().collect()).collect();
    let cols = lines.iter().map(Vec::len).max().unwrap_or(0);
    if cols > 240 || lines.len() > 240 { return Err(invalid()); }
    let mut out = start(40 + cols * 10, 40 + lines.len() * 20);
    for (row, line) in lines.iter().enumerate() {
        for (col, ch) in line.iter().enumerate() {
            let x = 25 + col * 10; let y = 25 + row * 20;
            let path = match ch {
                '-' | '=' => Some(format!("M {} {y} h 10", x - 5)),
                '|' | ':' => Some(format!("M {x} {} v 20", y - 10)),
                '+' => Some(format!("M {} {y} h 10 M {x} {} v 20", x - 5, y - 10)),
                '/' => Some(format!("M {} {} l 10 -20", x - 5, y + 10)),
                '\\' => Some(format!("M {} {} l 10 20", x - 5, y - 10)),
                _ => None,
            };
            if let Some(path) = path { out.push_str(&format!("<path d=\"{path}\" fill=\"none\" stroke=\"#3b5bdb\" stroke-width=\"2\"/>",)); }
            else if !ch.is_whitespace() { out.push_str(&format!("<text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-family=\"monospace\" font-size=\"14\" fill=\"#18191c\">{}</text>", y + 5, esc(&ch.to_string()))); }
        }
    }
    out.push_str("</svg>"); Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_diagrams_render_connections_and_escape_source() {
        let wsd = render("wsd", "title Login\nparticipant \"Reader\" as A\nA->Server: Request\nServer-->A: <script>reply</script>\nnote over A: Ready").unwrap();
        assert!(wsd.contains("stroke-dasharray")); assert!(wsd.contains("&lt;script&gt;")); assert!(!wsd.contains("<script>"));
        let graph = render("d2", "direction: right\na: Reader\nb: Writer\na -> b: Convert\nb -> a: Response\nb.shape: diamond").unwrap();
        assert!(graph.contains("<polygon")); assert!(graph.contains("Convert"));
        let ascii = render("ditaa", "+-----+\n| Box |----> Output\n+-----+").unwrap();
        assert!(ascii.contains("<path")); assert!(!ascii.contains("<image"));
        assert!(render("d2", "a: {shape: cloud}").is_err());
        assert!(render("wsd", "unrecognised language directive").is_err());
    }
}
