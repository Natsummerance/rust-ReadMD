//! Optional text projection must never take a successfully generated PDF
//! preview down. Some valid predefined CID encodings are unsupported by the
//! richer extractor; use the PDF's own ToUnicode map through lopdf in that case.
pub fn extract(bytes: &[u8]) -> Result<String, String> {
    let doc = lopdf::Document::load_mem(bytes).map_err(|_| "pdf_invalid")?;
    let unsupported = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .any(|d| {
            d.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Type0")
                && d.get(b"Encoding")
                    .ok()
                    .and_then(|v| v.as_name().ok())
                    .is_some_and(|v| v != b"Identity-H" && v != b"Identity-V")
        });
    if !unsupported {
        if let Ok(Ok(text)) = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes))
        {
            return Ok(text);
        }
    }
    let pages: Vec<_> = doc.get_pages().into_keys().collect();
    // This parsed document is local to the projection and is discarded even
    // when an extractor panics. No mutable document state escapes the boundary.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&pages)))
        .map_err(|_| "pdf_text_unavailable")?
        .map_err(|_| "pdf_text_unavailable".into())
}

/// A narrow check for our generated PDFs, never an imported-PDF validator.
/// ToUnicode/ActualText may legitimately contain U+FFFD and do not establish
/// which glyph was painted. Inspect only used Identity CID text-show operands.
pub(crate) fn validate_generated_glyphs(doc: &lopdf::Document) -> Result<(), &'static str> {
    generated_glyphs::validate(doc)
}

mod generated_glyphs {
    use lopdf::{content::Content, Dictionary, Document, Object, Stream};
    use std::{
        collections::{HashMap, HashSet},
        io::Read,
        sync::Arc,
    };

    const INVALID: &str = "typst_invalid_output";
    const MISSING: &str = "typst_invalid_content";
    const DEPTH: usize = 32;
    const OPERATIONS: usize = 250_000;
    const CONTENT_BYTES: usize = 8 * 1024 * 1024;
    const TOTAL_BYTES: usize = 64 * 1024 * 1024;
    const MAP_BYTES: usize = 2 * 65_536;

    enum Mapping {
        OtherEncoding,
        Identity,
        Table(Arc<Vec<u8>>),
    }

    fn resolve<'a>(doc: &'a Document, mut value: &'a Object) -> Result<&'a Object, &'static str> {
        for _ in 0..DEPTH {
            match value {
                Object::Reference(id) => value = doc.objects.get(id).ok_or(INVALID)?,
                _ => return Ok(value),
            }
        }
        Err(INVALID)
    }

    fn number(value: &Object) -> bool {
        matches!(value, Object::Integer(_))
            || matches!(value, Object::Real(value) if value.is_finite())
    }

    struct Guard<'a> {
        doc: &'a Document,
        fonts: HashMap<usize, Mapping>,
        streams: HashMap<usize, Arc<Vec<u8>>>,
        maps: HashSet<usize>,
        bytes: usize,
        map_bytes: usize,
        shown_bytes: usize,
        operations: usize,
        nodes: usize,
    }

    impl<'a> Guard<'a> {
        fn stream(
            &mut self,
            stream: &'a Stream,
            limit: usize,
        ) -> Result<Arc<Vec<u8>>, &'static str> {
            let key = stream as *const Stream as usize;
            if let Some(bytes) = self.streams.get(&key) {
                return if bytes.len() <= limit {
                    Ok(bytes.clone())
                } else {
                    Err(INVALID)
                };
            }
            if stream.content.len() > CONTENT_BYTES {
                return Err(INVALID);
            }
            let filter = match stream
                .dict
                .get(b"Filter")
                .ok()
                .map(|v| resolve(self.doc, v))
                .transpose()?
            {
                None | Some(Object::Null) => None,
                Some(Object::Name(name)) => Some(name.as_slice()),
                Some(Object::Array(values)) if values.is_empty() => None,
                Some(Object::Array(values)) if values.len() == 1 => Some(
                    resolve(self.doc, &values[0])?
                        .as_name()
                        .map_err(|_| INVALID)?,
                ),
                _ => return Err(INVALID),
            };
            let bytes = if let Some(filter) = filter {
                // These are the filters emitted by our generators. Other PDF
                // encodings/fonts are outside this guard's certification scope.
                if filter != b"FlateDecode" {
                    return Err(INVALID);
                }
                if let Some(params) = stream.dict.get(b"DecodeParms").ok() {
                    let mut params = resolve(self.doc, params)?;
                    if let Object::Array(values) = params {
                        if values.len() != 1 {
                            return Err(INVALID);
                        }
                        params = resolve(self.doc, &values[0])?;
                    }
                    if !matches!(params, Object::Null) {
                        let dict = params.as_dict().map_err(|_| INVALID)?;
                        if let Ok(predictor) = dict.get(b"Predictor") {
                            if resolve(self.doc, predictor)?
                                .as_i64()
                                .map_err(|_| INVALID)?
                                != 1
                            {
                                return Err(INVALID);
                            }
                        }
                    }
                }
                let mut bytes = Vec::new();
                flate2::read::ZlibDecoder::new(stream.content.as_slice())
                    .take((limit + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|_| INVALID)?;
                bytes
            } else {
                if stream.content.len() > limit {
                    return Err(INVALID);
                }
                stream.content.clone()
            };
            if bytes.len() > limit {
                return Err(INVALID);
            }
            self.bytes = self.bytes.checked_add(bytes.len()).ok_or(INVALID)?;
            if self.bytes > TOTAL_BYTES {
                return Err(INVALID);
            }
            let bytes = Arc::new(bytes);
            self.streams.insert(key, bytes.clone());
            Ok(bytes)
        }

        fn resources(
            &self,
            dict: &'a Dictionary,
            inherited: Option<&'a Dictionary>,
        ) -> Result<Option<&'a Dictionary>, &'static str> {
            match dict
                .get(b"Resources")
                .ok()
                .map(|v| resolve(self.doc, v))
                .transpose()?
            {
                None | Some(Object::Null) => Ok(inherited),
                Some(value) => Ok(Some(value.as_dict().map_err(|_| INVALID)?)),
            }
        }

        fn resource(
            &self,
            resources: Option<&'a Dictionary>,
            kind: &[u8],
            name: &[u8],
        ) -> Result<&'a Object, &'static str> {
            let entries = resources.ok_or(INVALID)?.get(kind).map_err(|_| INVALID)?;
            resolve(self.doc, entries)?
                .as_dict()
                .map_err(|_| INVALID)?
                .get(name)
                .map_err(|_| INVALID)
        }

        fn mapping(&mut self, font: &'a Object) -> Result<usize, &'static str> {
            let font = resolve(self.doc, font)?;
            let key = font as *const Object as usize;
            if self.fonts.contains_key(&key) {
                return Ok(key);
            }
            if self.fonts.len() >= 4096 {
                return Err(INVALID);
            }
            let dict = font.as_dict().map_err(|_| INVALID)?;
            let subtype = dict
                .get(b"Subtype")
                .map_err(|_| INVALID)?
                .as_name()
                .map_err(|_| INVALID)?;
            let mapping = if subtype != b"Type0" {
                Mapping::OtherEncoding
            } else {
                let encoding = resolve(self.doc, dict.get(b"Encoding").map_err(|_| INVALID)?)?;
                match encoding {
                    Object::Name(name) if name == b"Identity-H" || name == b"Identity-V" => {
                        let descendants =
                            resolve(self.doc, dict.get(b"DescendantFonts").map_err(|_| INVALID)?)?
                                .as_array()
                                .map_err(|_| INVALID)?;
                        if descendants.len() != 1 {
                            return Err(INVALID);
                        }
                        let cid = resolve(self.doc, &descendants[0])?
                            .as_dict()
                            .map_err(|_| INVALID)?;
                        match cid
                            .get(b"Subtype")
                            .map_err(|_| INVALID)?
                            .as_name()
                            .map_err(|_| INVALID)?
                        {
                            // In a CID-keyed CFF, CID zero denotes .notdef.
                            b"CIDFontType0" => Mapping::Identity,
                            b"CIDFontType2" => match cid
                                .get(b"CIDToGIDMap")
                                .ok()
                                .map(|v| resolve(self.doc, v))
                                .transpose()?
                            {
                                None | Some(Object::Null) => Mapping::Identity,
                                Some(Object::Name(name)) if name == b"Identity" => {
                                    Mapping::Identity
                                }
                                Some(Object::Stream(stream)) => {
                                    let bytes = self.stream(stream, MAP_BYTES)?;
                                    if bytes.len() % 2 != 0 {
                                        return Err(INVALID);
                                    }
                                    if self.maps.insert(stream as *const Stream as usize) {
                                        self.map_bytes = self
                                            .map_bytes
                                            .checked_add(bytes.len())
                                            .ok_or(INVALID)?;
                                    }
                                    if self.map_bytes > 8 * 1024 * 1024 {
                                        return Err(INVALID);
                                    }
                                    Mapping::Table(bytes)
                                }
                                _ => return Err(INVALID),
                            },
                            _ => return Err(INVALID),
                        }
                    }
                    // Predefined and custom CMaps need a separate interpreter.
                    // Do not interpret their character codes as Identity CIDs.
                    Object::Name(_) | Object::Stream(_) => Mapping::OtherEncoding,
                    _ => return Err(INVALID),
                }
            };
            self.fonts.insert(key, mapping);
            Ok(key)
        }

        fn text(&mut self, font: Option<&'a Object>, value: &Object) -> Result<(), &'static str> {
            let bytes = value.as_str().map_err(|_| INVALID)?;
            if bytes.is_empty() {
                return Ok(());
            }
            self.shown_bytes = self.shown_bytes.checked_add(bytes.len()).ok_or(INVALID)?;
            if self.shown_bytes > TOTAL_BYTES {
                return Err(INVALID);
            }
            let key = self.mapping(font.ok_or(INVALID)?)?;
            let mapping = self.fonts.get(&key).ok_or(INVALID)?;
            if matches!(mapping, Mapping::OtherEncoding) {
                return Ok(());
            }
            if bytes.len() % 2 != 0 {
                return Err(INVALID);
            }
            for pair in bytes.chunks_exact(2) {
                let cid = u16::from_be_bytes([pair[0], pair[1]]);
                let gid = match mapping {
                    Mapping::Identity => cid,
                    Mapping::Table(map) => {
                        let index = usize::from(cid) * 2;
                        map.get(index..index + 2)
                            .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
                    }
                    Mapping::OtherEncoding => unreachable!(),
                };
                if gid == 0 {
                    return Err(MISSING);
                }
            }
            Ok(())
        }

        fn contents(
            &mut self,
            value: &'a Object,
            bytes: &mut Vec<u8>,
            depth: usize,
        ) -> Result<(), &'static str> {
            if depth >= DEPTH {
                return Err(INVALID);
            }
            match resolve(self.doc, value)? {
                Object::Null => (),
                Object::Stream(stream) => {
                    let data = self.stream(stream, CONTENT_BYTES)?;
                    if bytes.len().saturating_add(data.len()).saturating_add(1) > CONTENT_BYTES {
                        return Err(INVALID);
                    }
                    bytes.extend_from_slice(&data);
                    bytes.push(b'\n');
                }
                Object::Array(values) => {
                    for value in values {
                        self.contents(value, bytes, depth + 1)?;
                    }
                }
                _ => return Err(INVALID),
            }
            Ok(())
        }

        fn paint(
            &mut self,
            bytes: &[u8],
            resources: Option<&'a Dictionary>,
            mut font: Option<&'a Object>,
            depth: usize,
        ) -> Result<(), &'static str> {
            if depth >= DEPTH {
                return Err(INVALID);
            }
            let content = Content::decode_strict(bytes).map_err(|_| INVALID)?;
            self.operations = self
                .operations
                .checked_add(content.operations.len())
                .ok_or(INVALID)?;
            if self.operations > OPERATIONS {
                return Err(INVALID);
            }
            let mut stack = Vec::new();
            for op in content.operations {
                let args = op.operands.as_slice();
                match op.operator.as_str() {
                    "q" if args.is_empty() => {
                        if stack.len() >= DEPTH {
                            return Err(INVALID);
                        }
                        stack.push(font);
                    }
                    "Q" if args.is_empty() => font = stack.pop().ok_or(INVALID)?,
                    "Tf" if args.len() == 2 && number(&args[1]) => {
                        font = Some(self.resource(
                            resources,
                            b"Font",
                            args[0].as_name().map_err(|_| INVALID)?,
                        )?)
                    }
                    "Tj" | "'" if args.len() == 1 => self.text(font, &args[0])?,
                    "\"" if args.len() == 3 && number(&args[0]) && number(&args[1]) => {
                        self.text(font, &args[2])?
                    }
                    "TJ" if args.len() == 1 => {
                        for value in args[0].as_array().map_err(|_| INVALID)? {
                            if !number(value) {
                                self.text(font, value)?;
                            }
                        }
                    }
                    "Do" if args.len() == 1 => {
                        let object = self.resource(
                            resources,
                            b"XObject",
                            args[0].as_name().map_err(|_| INVALID)?,
                        )?;
                        let stream = resolve(self.doc, object)?
                            .as_stream()
                            .map_err(|_| INVALID)?;
                        let subtype = stream
                            .dict
                            .get(b"Subtype")
                            .map_err(|_| INVALID)?
                            .as_name()
                            .map_err(|_| INVALID)?;
                        if subtype == b"Form" {
                            let child_resources = self.resources(&stream.dict, resources)?;
                            let data = self.stream(stream, CONTENT_BYTES)?;
                            // Form invocation saves/restores the caller state;
                            // inherited font is an object, not a re-looked-up name.
                            self.paint(&data, child_resources, font, depth + 1)?;
                        }
                    }
                    "q" | "Q" | "Tf" | "Tj" | "TJ" | "'" | "\"" | "Do" => return Err(INVALID),
                    _ => (),
                }
            }
            if !stack.is_empty() {
                return Err(INVALID);
            }
            Ok(())
        }

        fn pages(
            &mut self,
            value: &'a Object,
            inherited: Option<&'a Dictionary>,
            depth: usize,
        ) -> Result<(), &'static str> {
            if depth >= DEPTH {
                return Err(INVALID);
            }
            self.nodes += 1;
            if self.nodes > 16_384 {
                return Err(INVALID);
            }
            let dict = resolve(self.doc, value)?.as_dict().map_err(|_| INVALID)?;
            let resources = self.resources(dict, inherited)?;
            match dict
                .get(b"Type")
                .map_err(|_| INVALID)?
                .as_name()
                .map_err(|_| INVALID)?
            {
                b"Pages" => {
                    let kids = resolve(self.doc, dict.get(b"Kids").map_err(|_| INVALID)?)?
                        .as_array()
                        .map_err(|_| INVALID)?;
                    for child in kids {
                        self.pages(child, resources, depth + 1)?;
                    }
                }
                b"Page" => {
                    if let Ok(contents) = dict.get(b"Contents") {
                        let mut bytes = Vec::new();
                        self.contents(contents, &mut bytes, 0)?;
                        self.paint(&bytes, resources, None, 0)?;
                    }
                }
                _ => return Err(INVALID),
            }
            Ok(())
        }
    }

    pub(super) fn validate(doc: &Document) -> Result<(), &'static str> {
        let root = resolve(doc, doc.trailer.get(b"Root").map_err(|_| INVALID)?)?
            .as_dict()
            .map_err(|_| INVALID)?;
        let mut guard = Guard {
            doc,
            fonts: HashMap::new(),
            streams: HashMap::new(),
            maps: HashSet::new(),
            bytes: 0,
            map_bytes: 0,
            shown_bytes: 0,
            operations: 0,
            nodes: 0,
        };
        guard.pages(root.get(b"Pages").map_err(|_| INVALID)?, None, 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::pdf_render::{
        DocGeometry, DrawItem, Face, RenderedDoc, RenderedPage, Rgb, TextSpan,
    };
    use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

    #[test]
    fn predefined_cjk_fallback_has_searchable_text_without_system_fonts() {
        let text = "目录：证据与判断";
        let geom = DocGeometry::from_style(&crate::export_styles::sanitize(&serde_json::json!({})));
        let page = RenderedPage {
            w: geom.page_w,
            h: geom.page_h,
            number: 1,
            text: text.into(),
            items: vec![DrawItem::Text {
                x: 40.0,
                y: 700.0,
                text: text.into(),
                spans: vec![TextSpan {
                    face: Face::Cjk,
                    size: 12.0,
                    color: Rgb::rgb(0.0, 0.0, 0.0),
                    bytes: text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                    advance: 108.0,
                }],
            }],
        };
        let bytes = crate::pdf_render::write_pdf(&RenderedDoc {
            geom,
            pages: vec![page],
            outlines: vec![],
            toc_entries: vec![],
            producer_face: "STSong-Light".into(),
            embedded: Default::default(),
        });
        let parsed = lopdf::Document::load_mem(&bytes).unwrap();
        let font = parsed
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| d.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Type0"))
            .unwrap();
        assert!(font.get(b"ToUnicode").is_ok());
        assert_eq!(
            font.get(b"Encoding").unwrap().as_name().unwrap(),
            b"UniGB-UCS2-H"
        );
        assert!(super::extract(&bytes).unwrap().contains(text));
    }

    #[test]
    fn invalid_pdf_is_a_projection_error() {
        assert_eq!(super::extract(b"not a PDF").unwrap_err(), "pdf_invalid");
    }

    fn generated_font(doc: &mut Document, subtype: &str, map: Option<Vec<u8>>) -> ObjectId {
        let mut cid = dictionary! {"Type"=>"Font", "Subtype"=>subtype, "BaseFont"=>"Synthetic",
        "CIDSystemInfo"=>dictionary! {"Registry"=>Object::string_literal("Adobe"),"Ordering"=>Object::string_literal("Identity"),"Supplement"=>0}};
        if let Some(map) = map {
            let map = doc.add_object(Stream::new(Dictionary::new(), map));
            cid.set("CIDToGIDMap", map);
        }
        let cid = doc.add_object(cid);
        doc.add_object(
            dictionary! {"Type"=>"Font","Subtype"=>"Type0","BaseFont"=>"Synthetic",
            "Encoding"=>"Identity-H","DescendantFonts"=>vec![Object::Reference(cid)]},
        )
    }

    fn generated_document(
        text: &[u8],
        subtype: &str,
        map: Option<Vec<u8>>,
    ) -> (Document, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let font = generated_font(&mut doc, subtype, map);
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(Dictionary::new(), text.to_vec()));
        let page = doc.add_object(
            dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>content,
            "MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()]},
        );
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1,
            "Resources"=>dictionary! {"Font"=>dictionary! {"F"=>font}}}
            .into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        (doc, page, font)
    }

    fn page_resources(doc: &mut Document, page: ObjectId) -> &mut Dictionary {
        let parent = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_dictionary_mut(parent)
            .unwrap()
            .get_mut(b"Resources")
            .unwrap()
            .as_dict_mut()
            .unwrap()
    }

    #[test]
    fn generated_identity_fonts_reject_drawn_notdef_and_accept_nonzero_cids() {
        for subtype in ["CIDFontType0", "CIDFontType2"] {
            let (doc, _, _) = generated_document(b"BT /F 12 Tf <0000> Tj ET", subtype, None);
            assert_eq!(
                super::validate_generated_glyphs(&doc),
                Err("typst_invalid_content")
            );
            let (mut doc, _, font) = generated_document(b"BT /F 12 Tf <0001> Tj ET", subtype, None);
            doc.get_dictionary_mut(font)
                .unwrap()
                .set("Encoding", "Identity-V");
            assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
        }
    }

    #[test]
    fn generated_unused_zero_mapping_does_not_reject_a_real_drawn_glyph() {
        let (mut doc, page, _) = generated_document(
            b"BT /F 12 Tf <0001> Tj ET",
            "CIDFontType2",
            Some(vec![0, 0, 0, 7, 0, 0]),
        );
        let unused = generated_font(&mut doc, "CIDFontType2", Some(vec![0, 0, 0, 0]));
        page_resources(&mut doc, page)
            .get_mut(b"Font")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Unused", unused);
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
    }

    #[test]
    fn generated_cid_to_gid_map_checks_actual_gid_instead_of_cid_zero() {
        let (doc, _, _) = generated_document(
            b"BT /F 12 Tf <0001> Tj ET",
            "CIDFontType2",
            Some(vec![0, 0, 0, 0]),
        );
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_content")
        );
        let (doc, _, _) = generated_document(
            b"BT /F 12 Tf <0000> Tj ET",
            "CIDFontType2",
            Some(vec![0, 7]),
        );
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
        let (mut doc, _, font) =
            generated_document(b"BT /F 12 Tf <0000> Tj ET", "CIDFontType2", None);
        let cid = doc
            .get_dictionary(font)
            .unwrap()
            .get(b"DescendantFonts")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        doc.get_dictionary_mut(cid)
            .unwrap()
            .set("CIDToGIDMap", "Identity");
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_content")
        );
    }

    #[test]
    fn generated_all_four_text_show_operators_check_used_glyphs() {
        for operator in [
            "<0000> Tj",
            "[12 <0000> -8 <0001>] TJ",
            "<0000> '",
            "1 2 <0000> \"",
        ] {
            let source = format!("BT /F 12 Tf {operator} ET");
            let (doc, _, _) = generated_document(source.as_bytes(), "CIDFontType2", None);
            assert_eq!(
                super::validate_generated_glyphs(&doc),
                Err("typst_invalid_content"),
                "{operator}"
            );
            let (doc, _, _) = generated_document(
                source.replace("0000", "0001").as_bytes(),
                "CIDFontType2",
                None,
            );
            assert_eq!(super::validate_generated_glyphs(&doc), Ok(()), "{operator}");
        }
    }

    #[test]
    fn generated_author_fffd_with_real_gid_is_not_a_missing_glyph() {
        let (mut doc, page, font) = generated_document(
            b"BT /F 12 Tf <0001> Tj ET",
            "CIDFontType2",
            Some(vec![0, 0, 0, 7]),
        );
        let unicode = doc.add_object(Stream::new(Dictionary::new(), b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Author def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <0001> <FFFD> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end".to_vec()));
        doc.get_dictionary_mut(font)
            .unwrap()
            .set("ToUnicode", unicode);
        // lopdf's extractor discovers ancestor resources only by reference.
        // Keep the same inherited dictionary but make that object explicit;
        // this test must exercise both the real CMap and the glyph guard.
        let resources = page_resources(&mut doc, page).clone();
        let resources = doc.add_object(resources);
        let parent = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_dictionary_mut(parent)
            .unwrap()
            .set("Resources", resources);
        let encoding = doc
            .get_dictionary(font)
            .unwrap()
            .get_font_encoding(&doc)
            .unwrap();
        assert_eq!(
            Document::decode_text(&encoding, &[0, 1]).unwrap(),
            "\u{FFFD}"
        );
        let extracted = doc.extract_text(&[1]).unwrap();
        assert!(
            extracted.contains('\u{FFFD}'),
            "actual extraction: {extracted:?}"
        );
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
    }

    #[test]
    fn generated_page_resources_and_q_scope_cross_content_stream_boundaries() {
        let (mut doc, page, _) =
            generated_document(b"BT /F 12 Tf q /B 12 Tf", "CIDFontType2", None);
        let bad = generated_font(&mut doc, "CIDFontType2", Some(vec![0, 0, 0, 0]));
        page_resources(&mut doc, page)
            .get_mut(b"Font")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("B", bad);
        let first = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .clone();
        let second = doc.add_object(Stream::new(Dictionary::new(), b"Q <0001> Tj ET".to_vec()));
        doc.get_dictionary_mut(page)
            .unwrap()
            .set("Contents", vec![first, Object::Reference(second)]);
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
        doc.get_object_mut(second)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_plain_content(b"<0001> Tj Q ET".to_vec());
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_content")
        );
    }

    #[test]
    fn generated_form_font_inheritance_is_by_object_and_local_selection_is_restored() {
        let (mut doc, page, _) = generated_document(
            b"BT /F 12 Tf ET /X Do BT <0001> Tj ET",
            "CIDFontType2",
            None,
        );
        let bad = generated_font(&mut doc, "CIDFontType2", Some(vec![0, 0, 0, 0]));
        let form = doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),100.into()],
            "Resources"=>dictionary! {"Font"=>dictionary! {"F"=>bad}}}, b"BT <0001> Tj /F 12 Tf ET".to_vec()));
        page_resources(&mut doc, page).set("XObject", dictionary! {"X"=>form});
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
        doc.get_object_mut(form)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_plain_content(b"BT /F 12 Tf <0001> Tj ET".to_vec());
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_content")
        );
        doc.get_object_mut(form)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .dict
            .remove(b"Resources");
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
    }

    #[test]
    fn generated_other_legal_cmap_encodings_are_not_misread_as_identity() {
        let (mut doc, _, font) = generated_document(
            b"BT /F 12 Tf (A) Tj ET",
            "CIDFontType2",
            Some(vec![0, 0, 0, 0]),
        );
        doc.get_dictionary_mut(font)
            .unwrap()
            .set("Encoding", "90ms-RKSJ-H");
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
    }

    #[test]
    fn generated_malformed_identity_code_and_oversized_map_fail_as_invalid_output() {
        let (doc, _, _) = generated_document(b"BT /F 12 Tf <01> Tj ET", "CIDFontType2", None);
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_output")
        );
        let (mut doc, _, font) = generated_document(
            b"BT /F 12 Tf <0001> Tj ET",
            "CIDFontType2",
            Some(vec![0; 131_074]),
        );
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_output")
        );
        let cid = doc
            .get_dictionary(font)
            .unwrap()
            .get(b"DescendantFonts")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        let map = doc
            .get_dictionary(cid)
            .unwrap()
            .get(b"CIDToGIDMap")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_object_mut(map)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .compress()
            .unwrap();
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_output")
        );
    }

    #[test]
    fn generated_recursive_form_is_bounded_without_visiting_unpainted_forms() {
        let (mut doc, page, _) = generated_document(b"/X Do", "CIDFontType2", None);
        let form = doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),1.into(),1.into()]}, b"/X Do".to_vec()));
        page_resources(&mut doc, page).set("XObject", dictionary! {"X"=>form});
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_output")
        );
        let content = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_object_mut(content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_plain_content(Vec::new());
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
    }

    #[test]
    fn generated_operation_budget_counts_painted_forms_only() {
        let (mut doc, page, _) = generated_document(b"", "CIDFontType2", None);
        let form = doc.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),1.into(),1.into()]}, b"0 0 m\n".repeat(250_001)));
        page_resources(&mut doc, page).set("XObject", dictionary! {"X"=>form});
        assert_eq!(super::validate_generated_glyphs(&doc), Ok(()));
        let content = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_object_mut(content)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_plain_content(b"/X Do".to_vec());
        assert_eq!(
            super::validate_generated_glyphs(&doc),
            Err("typst_invalid_output")
        );
    }
}
