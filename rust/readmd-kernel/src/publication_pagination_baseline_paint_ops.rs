use super::*;

#[derive(Default)]
struct Path {
    bounds: Option<Bounds>,
    rect: Option<Bounds>,
    current: Option<(f64, f64)>,
    start: Option<(f64, f64)>,
    clip: bool,
    polygon: Vec<(f64, f64)>,
    rectangle: bool,
}
impl Path {
    fn point(&mut self, state: &State, x: f64, y: f64) -> Result<(f64, f64), String> {
        let p = state.ctm.point(x, y);
        if !p.0.is_finite() || !p.1.is_finite() {
            return Err(INVALID.into());
        }
        if let Some(b) = &mut self.bounds {
            b.add(p.0, p.1);
        } else {
            self.bounds = Some(Bounds::point(p.0, p.1));
        }
        Ok(p)
    }
    fn finish(&mut self, state: &mut State) -> Result<(), String> {
        if self.clip {
            let r = self.rect.ok_or(UNSUPPORTED)?;
            state.clip = state.clip.intersection(r)?;
        }
        *self = Self::default();
        Ok(())
    }
    fn close(&mut self) {
        if self.rectangle
            && self.polygon.len() == 4
            && self.polygon.iter().enumerate().all(|(i, p)| {
                let q = self.polygon[(i + 1) % 4];
                (p.0 - q.0).abs() < 1e-9 || (p.1 - q.1).abs() < 1e-9
            })
        {
            self.rect = self.bounds;
        }
        self.current = self.start;
    }
}

impl<'a, 'b> Reader<'a, 'b> {
    fn ext_state(
        &mut self,
        state: &mut State,
        resources: &'a Dictionary,
        name: &[u8],
    ) -> Result<(), String> {
        let d = dictionary(self.doc, resource(self.doc, resources, b"ExtGState", name)?)?;
        for (key, value) in d.iter() {
            match key.as_slice() {
                b"Type" => {
                    if value.as_name().ok() != Some(b"ExtGState") {
                        return Err(INVALID.into());
                    }
                }
                b"ca" | b"CA" => {
                    let alpha = number(value)?;
                    if !(0. ..=1.).contains(&alpha) {
                        return Err(INVALID.into());
                    }
                    if key == b"ca" {
                        state.fill_alpha = alpha;
                    } else {
                        state.stroke_alpha = alpha;
                    }
                }
                b"BM" => {
                    if direct(self.doc, value)?.as_name().ok() != Some(b"Normal") {
                        return Err(UNSUPPORTED.into());
                    }
                }
                b"SMask" => {
                    if direct(self.doc, value)?.as_name().ok() != Some(b"None") {
                        return Err(UNSUPPORTED.into());
                    }
                }
                b"LW" => {
                    state.width = number(value)?;
                    if state.width < 0. {
                        return Err(INVALID.into());
                    }
                }
                b"LC" | b"LJ" => {
                    let n = value.as_i64().map_err(|_| INVALID)?;
                    if !(0..=2).contains(&n) {
                        return Err(INVALID.into());
                    }
                    if key == b"LC" {
                        state.cap = n;
                    } else {
                        state.join = n;
                    }
                }
                b"ML" => {
                    state.miter = number(value)?;
                    if state.miter < 1. {
                        return Err(INVALID.into());
                    }
                }
                b"SA" | b"OP" | b"op" | b"AIS" => {
                    if value.as_bool().ok() != Some(false) {
                        return Err(UNSUPPORTED.into());
                    }
                }
                b"TK" => {
                    if value.as_bool().ok() != Some(true) {
                        return Err(UNSUPPORTED.into());
                    }
                }
                b"OPM" => {
                    if number(value)? != 0. {
                        return Err(UNSUPPORTED.into());
                    }
                }
                _ => return Err(UNSUPPORTED.into()),
            }
        }
        Ok(())
    }
    fn colorspace(&self, resources: &Dictionary, name: &[u8]) -> Result<(), String> {
        if [
            b"DeviceRGB".as_slice(),
            b"DeviceGray".as_slice(),
            b"DeviceCMYK".as_slice(),
        ]
        .contains(&name)
        {
            return Ok(());
        }
        let value = direct(
            self.doc,
            resource(self.doc, resources, b"ColorSpace", name)?,
        )?;
        if value.as_name().ok() == Some(b"Pattern")
            || value
                .as_array()
                .ok()
                .is_some_and(|a| a.first().and_then(|v| v.as_name().ok()) == Some(b"Pattern"))
        {
            return Err(UNSUPPORTED.into());
        }
        Ok(())
    }
    pub(super) fn paint(
        &mut self,
        page: ObjectId,
        bytes: &[u8],
        resources: &'a Dictionary,
        mut state: State,
        mut marks: Vec<Mark>,
        depth: usize,
        in_form: bool,
    ) -> Result<(), String> {
        self.budget.step(depth)?;
        let content = PdfContent::decode_strict(bytes).map_err(|_| INVALID)?;
        self.operations = self
            .operations
            .checked_add(content.operations.len())
            .ok_or(LIMIT)?;
        if self.operations > MAX_OPS {
            return Err(LIMIT.into());
        }
        let mut saved = Vec::new();
        let mut path = Path::default();
        let mut in_text = false;
        let mark_base = marks.len();
        for op in content.operations {
            self.budget.step(depth)?;
            let a = op.operands.as_slice();
            match op.operator.as_str() {
                "BDC" => {
                    if a.len() != 2 {
                        return Err(INVALID.into());
                    }
                    let tag = a[0].as_name().map_err(|_| INVALID)?;
                    let property = if let Object::Name(name) = &a[1] {
                        resource(self.doc, resources, b"Properties", name)?
                    } else {
                        &a[1]
                    };
                    let d = dictionary(self.doc, property)?;
                    if let Ok(v) = d.get(b"MCID") {
                        if in_form || marks.iter().any(|m| matches!(m, Mark::Content(..))) {
                            return Err(UNSUPPORTED.into());
                        }
                        let mcid = v.as_i64().map_err(|_| INVALID)?;
                        if mcid < 0
                            || !self.sites.contains_key(&(page, mcid))
                            || !self.seen.insert((page, mcid))
                        {
                            return Err(INVALID.into());
                        }
                        if tag == b"OC" || tag == b"Artifact" {
                            return Err(UNSUPPORTED.into());
                        }
                        marks.push(Mark::Content(page, mcid));
                    } else if tag == b"Artifact" {
                        if marks.iter().any(|m| matches!(m, Mark::Content(..))) {
                            return Err(UNSUPPORTED.into());
                        }
                        marks.push(Mark::Artifact);
                    } else {
                        return Err(UNSUPPORTED.into());
                    }
                    if marks.len() > MAX_DEPTH {
                        return Err(LIMIT.into());
                    }
                }
                "BMC" => {
                    if a.len() != 1
                        || a[0].as_name().ok() != Some(b"Artifact")
                        || marks.iter().any(|m| matches!(m, Mark::Content(..)))
                    {
                        return Err(UNSUPPORTED.into());
                    }
                    marks.push(Mark::Artifact);
                    if marks.len() > MAX_DEPTH {
                        return Err(LIMIT.into());
                    }
                }
                "EMC" => {
                    if !a.is_empty() || marks.len() <= mark_base {
                        return Err(INVALID.into());
                    }
                    marks.pop();
                }
                "MP" => {
                    if a.len() != 1 || a[0].as_name().ok() != Some(b"Artifact") {
                        return Err(UNSUPPORTED.into());
                    }
                }
                "q" => {
                    if !a.is_empty() || in_text {
                        return Err(INVALID.into());
                    }
                    if saved.len() >= MAX_DEPTH {
                        return Err(LIMIT.into());
                    }
                    saved.push(state.clone());
                }
                "Q" => {
                    if !a.is_empty() || in_text {
                        return Err(INVALID.into());
                    }
                    state = saved.pop().ok_or(INVALID)?;
                }
                "cm" => {
                    if in_text {
                        return Err(INVALID.into());
                    }
                    state.ctm = state.ctm.concat(matrix(a)?);
                    if !state.ctm.finite() {
                        return Err(INVALID.into());
                    }
                }
                "BT" => {
                    if !a.is_empty() || in_text {
                        return Err(INVALID.into());
                    }
                    in_text = true;
                    state.tm = Matrix::identity();
                    state.line = Matrix::identity();
                }
                "ET" => {
                    if !a.is_empty() || !in_text {
                        return Err(INVALID.into());
                    }
                    in_text = false;
                }
                "Tf" => {
                    if a.len() != 2 || !in_text {
                        return Err(INVALID.into());
                    }
                    state.size = number(&a[1])?;
                    if state.size <= 0. || state.size > 4096. {
                        return Err(UNSUPPORTED.into());
                    }
                    state.font = Some(self.font(resource(
                        self.doc,
                        resources,
                        b"Font",
                        a[0].as_name().map_err(|_| INVALID)?,
                    )?)?);
                }
                "Tm" => {
                    if !in_text {
                        return Err(INVALID.into());
                    }
                    state.tm = matrix(a)?;
                    state.line = state.tm;
                }
                "Td" | "TD" => {
                    if !in_text {
                        return Err(INVALID.into());
                    }
                    let n = numbers(a, 2)?;
                    state.line = state.line.translate(n[0], n[1]);
                    state.tm = state.line;
                    if op.operator == "TD" {
                        state.leading = -n[1];
                    }
                }
                "T*" => {
                    if !in_text || !a.is_empty() {
                        return Err(INVALID.into());
                    }
                    state.line = state.line.translate(0., -state.leading);
                    state.tm = state.line;
                }
                "Tc" | "Tw" | "Tz" | "TL" | "Ts" | "Tr" => {
                    if !in_text {
                        return Err(INVALID.into());
                    }
                    let n = numbers(a, 1)?[0];
                    match op.operator.as_str() {
                        "Tc" => state.char_space = n,
                        "Tw" => (),
                        "TL" => state.leading = n,
                        "Ts" => state.rise = n,
                        "Tz" => {
                            if n <= 0. || n > 10000. {
                                return Err(UNSUPPORTED.into());
                            }
                            state.hscale = n / 100.;
                        }
                        "Tr" => {
                            if n != 0. {
                                return Err(UNSUPPORTED.into());
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                "Tj" | "'" => {
                    if !in_text || a.len() != 1 {
                        return Err(INVALID.into());
                    }
                    if op.operator == "'" {
                        state.line = state.line.translate(0., -state.leading);
                        state.tm = state.line;
                    }
                    self.text(page, &mut state, &marks, &a[0])?;
                }
                "\"" => {
                    if !in_text || a.len() != 3 {
                        return Err(INVALID.into());
                    }
                    number(&a[0])?;
                    state.char_space = number(&a[1])?;
                    state.line = state.line.translate(0., -state.leading);
                    state.tm = state.line;
                    self.text(page, &mut state, &marks, &a[2])?;
                }
                "TJ" => {
                    if !in_text || a.len() != 1 {
                        return Err(INVALID.into());
                    }
                    for v in a[0].as_array().map_err(|_| INVALID)? {
                        if matches!(v, Object::String(..)) {
                            self.text(page, &mut state, &marks, v)?;
                        } else {
                            let n = number(v)?;
                            state.tm = state
                                .tm
                                .translate(-n / 1000. * state.size * state.hscale, 0.);
                        }
                    }
                }
                "w" => {
                    state.width = numbers(a, 1)?[0];
                    if state.width < 0. {
                        return Err(INVALID.into());
                    }
                }
                "J" | "j" => {
                    if a.len() != 1 {
                        return Err(INVALID.into());
                    }
                    let n = a[0].as_i64().map_err(|_| INVALID)?;
                    if !(0..=2).contains(&n) {
                        return Err(INVALID.into());
                    }
                    if op.operator == "J" {
                        state.cap = n;
                    } else {
                        state.join = n;
                    }
                }
                "M" => {
                    state.miter = numbers(a, 1)?[0];
                    if state.miter < 1. {
                        return Err(INVALID.into());
                    }
                }
                "d" => {
                    if a.len() != 2 {
                        return Err(INVALID.into());
                    }
                    let dash = a[0].as_array().map_err(|_| INVALID)?;
                    if dash.len() > 256
                        || dash
                            .iter()
                            .any(|n| number(n).is_err() || number(n).is_ok_and(|n| n < 0.))
                        || number(&a[1])? < 0.
                    {
                        return Err(INVALID.into());
                    }
                }
                "m" => {
                    if in_text {
                        return Err(INVALID.into());
                    }
                    let n = numbers(a, 2)?;
                    path.rectangle = path.bounds.is_none();
                    let p = path.point(&state, n[0], n[1])?;
                    path.current = Some(p);
                    path.start = Some(p);
                    if path.polygon.len() < 5 {
                        path.polygon.push(p);
                    } else {
                        path.rectangle = false;
                    }
                    path.rect = None;
                }
                "l" => {
                    if in_text || path.current.is_none() {
                        return Err(INVALID.into());
                    }
                    let n = numbers(a, 2)?;
                    let p = path.point(&state, n[0], n[1])?;
                    path.current = Some(p);
                    if path.polygon.len() < 5 {
                        path.polygon.push(p);
                    } else {
                        path.rectangle = false;
                    }
                    path.rect = None;
                }
                "c" | "v" | "y" => {
                    if in_text || path.current.is_none() {
                        return Err(INVALID.into());
                    }
                    let n = numbers(a, if op.operator == "c" { 6 } else { 4 })?;
                    for p in n.chunks_exact(2) {
                        path.current = Some(path.point(&state, p[0], p[1])?);
                    }
                    path.rect = None;
                    path.rectangle = false;
                }
                "h" => {
                    if in_text || !a.is_empty() || path.current.is_none() {
                        return Err(INVALID.into());
                    }
                    path.close();
                }
                "re" => {
                    if in_text {
                        return Err(INVALID.into());
                    }
                    let n = numbers(a, 4)?;
                    let first = path.bounds.is_none();
                    let mut r = None;
                    for (x, y) in [
                        (n[0], n[1]),
                        (n[0] + n[2], n[1]),
                        (n[0] + n[2], n[1] + n[3]),
                        (n[0], n[1] + n[3]),
                    ] {
                        let p = path.point(&state, x, y)?;
                        r = Some(r.map_or(Bounds::point(p.0, p.1), |b: Bounds| {
                            b.union(Bounds::point(p.0, p.1))
                        }));
                    }
                    path.current = Some(state.ctm.point(n[0], n[1]));
                    path.start = path.current;
                    path.rect = if first && state.ctm.axis_aligned() {
                        r
                    } else {
                        None
                    };
                }
                "W" | "W*" => {
                    if !a.is_empty() || path.bounds.is_none() {
                        return Err(INVALID.into());
                    }
                    if path.rect.is_none() {
                        return Err(UNSUPPORTED.into());
                    }
                    path.clip = true;
                }
                "n" => {
                    if !a.is_empty() {
                        return Err(INVALID.into());
                    }
                    path.finish(&mut state)?;
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                    if in_text || !a.is_empty() {
                        return Err(INVALID.into());
                    }
                    let mut rect = path.bounds.ok_or(INVALID)?;
                    let stroke =
                        matches!(op.operator.as_str(), "S" | "s" | "B" | "B*" | "b" | "b*")
                            && state.stroke_alpha > 0.;
                    let fill = matches!(
                        op.operator.as_str(),
                        "f" | "F" | "f*" | "B" | "B*" | "b" | "b*"
                    ) && state.fill_alpha > 0.;
                    if stroke {
                        if state.width <= 0. {
                            return Err(UNSUPPORTED.into());
                        }
                        let join = if state.join == 0 { state.miter } else { 1. };
                        let cap = if state.cap == 2 {
                            std::f64::consts::SQRT_2
                        } else {
                            1.
                        };
                        rect =
                            rect.expand(state.width / 2. * state.ctm.scale_bound() * join.max(cap));
                    }
                    if path.clip {
                        state.clip = state.clip.intersection(path.rect.ok_or(UNSUPPORTED)?)?;
                    }
                    if stroke || fill {
                        self.emit(page, &state, &marks, rect, None, false)?;
                    } else {
                        // Transparent Math SVG hotspots are known non-painting
                        // paths, not an exemption for fonts or unknown operators.
                        let site = match marks.last() {
                            Some(Mark::Content(p, m)) => {
                                self.sites.get(&(*p, *m)).ok_or(INVALID)?
                            }
                            Some(Mark::Artifact) => {
                                path = Path::default();
                                continue;
                            }
                            None => return Err(UNSUPPORTED.into()),
                        };
                        if site.rise != 0. && site.protected.is_none() {
                            return Err(UNSUPPORTED.into());
                        }
                    }
                    path = Path::default();
                }
                "Do" => {
                    if in_text || a.len() != 1 {
                        return Err(INVALID.into());
                    }
                    let object = direct(
                        self.doc,
                        resource(
                            self.doc,
                            resources,
                            b"XObject",
                            a[0].as_name().map_err(|_| INVALID)?,
                        )?,
                    )?;
                    let stream = object.as_stream().map_err(|_| INVALID)?;
                    if stream.dict.has(b"OC") {
                        return Err(UNSUPPORTED.into());
                    }
                    match stream
                        .dict
                        .get(b"Subtype")
                        .and_then(Object::as_name)
                        .map_err(|_| INVALID)?
                    {
                        b"Form" => {
                            let key = object as *const Object as usize;
                            if !self.forms.insert(key) {
                                return Err(INVALID.into());
                            }
                            let mut child = state.clone();
                            if let Ok(v) = stream.dict.get(b"Matrix") {
                                child.ctm = child.ctm.concat(matrix(
                                    direct(self.doc, v)?.as_array().map_err(|_| INVALID)?,
                                )?);
                            }
                            if !child.ctm.axis_aligned() {
                                return Err(UNSUPPORTED.into());
                            }
                            child.clip = child.clip.intersection(
                                bounds(self.doc, stream.dict.get(b"BBox").map_err(|_| INVALID)?)?
                                    .transformed(child.ctm),
                            )?;
                            let child_resources = match stream.dict.get(b"Resources") {
                                Ok(v) => dictionary(self.doc, v)?,
                                Err(_) => resources,
                            };
                            let data = self.stream(stream)?;
                            self.paint(
                                page,
                                &data,
                                child_resources,
                                child,
                                marks.clone(),
                                depth + 1,
                                true,
                            )?;
                            self.forms.remove(&key);
                        }
                        b"Image" => {
                            for key in [b"Width".as_slice(), b"Height".as_slice()] {
                                let n = stream
                                    .dict
                                    .get(key)
                                    .and_then(Object::as_i64)
                                    .map_err(|_| INVALID)?;
                                if !(1..=100000).contains(&n) {
                                    return Err(INVALID.into());
                                }
                            }
                            if stream.dict.has(b"SMask")
                                || stream.dict.has(b"Mask")
                                || stream
                                    .dict
                                    .get(b"ImageMask")
                                    .ok()
                                    .is_some_and(|v| v.as_bool().ok() != Some(false))
                            {
                                return Err(UNSUPPORTED.into());
                            }
                            self.emit(
                                page,
                                &state,
                                &marks,
                                Bounds([0., 0., 1., 1.]).transformed(state.ctm),
                                None,
                                false,
                            )?;
                        }
                        _ => return Err(UNSUPPORTED.into()),
                    }
                }
                "gs" => {
                    if a.len() != 1 {
                        return Err(INVALID.into());
                    }
                    self.ext_state(&mut state, resources, a[0].as_name().map_err(|_| INVALID)?)?;
                }
                "cs" | "CS" => {
                    if a.len() != 1 {
                        return Err(INVALID.into());
                    }
                    self.colorspace(resources, a[0].as_name().map_err(|_| INVALID)?)?;
                }
                "sc" | "SC" | "scn" | "SCN" => {
                    if a.is_empty() || a.len() > 32 {
                        return Err(INVALID.into());
                    }
                    for n in a {
                        number(n)?;
                    }
                }
                "g" | "G" => {
                    numbers(a, 1)?;
                }
                "rg" | "RG" => {
                    numbers(a, 3)?;
                }
                "k" | "K" => {
                    numbers(a, 4)?;
                }
                "ri" => {
                    if a.len() != 1 || a[0].as_name().is_err() {
                        return Err(INVALID.into());
                    }
                }
                "i" => {
                    numbers(a, 1)?;
                }
                _ => return Err(UNSUPPORTED.into()),
            }
        }
        if in_text || !saved.is_empty() || marks.len() != mark_base || path.bounds.is_some() {
            return Err(INVALID.into());
        }
        Ok(())
    }
}
