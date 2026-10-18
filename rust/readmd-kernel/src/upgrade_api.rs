//! Shared content operations; HTTP adapters contain no document-writing logic.
use crate::server::{ok_json, Request, Response};
use crate::{document_transaction, math_document, ApiError, ApiResult, App};
use serde_json::{json, Value};
use std::sync::Arc;

fn input(req: &Request) -> ApiResult<Value> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    req.json()
}
fn field<'a>(v: &'a Value, key: &str) -> ApiResult<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("invalid_json"))
}
fn math_error(code: &'static str) -> ApiError {
    ApiError::new(
        if code == "transaction_conflict" {
            409
        } else {
            422
        },
        code,
    )
}
fn math_preview_size(v:&Value)->ApiResult<f64>{
    if v.get("font_size").is_none(){return Ok(12.0);}
    v["font_size"].as_f64().filter(|size|size.is_finite()&&(4.0..=512.0).contains(size)).ok_or_else(||ApiError::bad_request("invalid_json"))
}
fn math_preview_width(v:&Value)->ApiResult<Option<f64>>{
    if v.get("layout_width").is_none(){return Ok(None);}
    v["layout_width"].as_f64().filter(|width|width.is_finite()&&(1.0..=20000.0).contains(width)).map(Some).ok_or_else(||ApiError::bad_request("invalid_json"))
}

fn math_vector_preview(doc:&math_document::MathDocument,display:bool,size:f64,width:Option<f64>)
    ->(Option<String>,Vec<crate::math_equation_array::EquationArrayDiagnostic>) {
    // One layout supplies both the visible paths and their source-bound
    // diagnostics. These projection issues never become author-source errors.
    match math_document::vector_preview_with_diagnostics(doc,display,size,width) {
        Some(result)=>(Some(result.svg),result.diagnostics),None=>(None,Vec::new()),
    }
}
pub fn academic_citations(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let request = crate::academic::parse_request(input(req)?)?;
    ok_json(crate::academic::citations(&request)?)
}
pub fn academic_document(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let v = input(req)?;
    let source=field(&v,"content")?;
    if v["view"].as_bool()==Some(true) { crate::publication_typography::validate_reader_view(source, &v["options"]).map_err(math_error)?; }
    let result =
        crate::publication::prepare_with_options(field(&v, "content")?, v["baseDir"].as_str().unwrap_or(""), &v["options"])?;
    let view=v["view"].as_bool()==Some(true);
    let displayed=if view {crate::cross_references::project(&result.projection.reader_content(source)?).content}else{result.content};
    let displayed=if view&&displayed.contains(crate::publication_sections::PREFIX){crate::mdexport::section_flow_source(&displayed,&crate::export_styles::sanitize(&v["options"])).map_err(|code|crate::ApiError::new(422,code))?}else{displayed};
    let mut data = result.data;
    if view&&displayed.contains(crate::publication_blocks::PREFIX){
        let (content,layout)=crate::publication_blocks::view(&displayed).map_err(math_error)?;
        data["content"]=json!(content);data["layout"]=layout;
    }else{data["content"] = json!(displayed);}
    data["warnings"] = json!(result.warnings);
    data["projection"] = json!(result.projection);
    ok_json(data)
}
pub fn publication_metadata(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let value = input(req)?;
    match crate::publication_metadata::handle(&value) {
        Ok(data) => ok_json(data),
        Err(error) => Ok(Response::json_status(
            if error.code.contains("conflict") || error.code.contains("revision") { 409 } else if error.code.ends_with("budget") { 413 } else { 422 },
            &error.response_body(),
        )),
    }
}
pub fn publication_blocks(_app:&Arc<App>,req:&Request)->ApiResult<Response>{
    let v=input(req)?;let source=field(&v,"content")?;
    match v["action"].as_str().unwrap_or("list"){
        "list"=>ok_json(json!({"ok":true,"document":crate::publication_blocks::analyze(source).map_err(math_error)?})),
        "apply"=>ok_json(crate::publication_blocks::apply(source,field(&v,"base_revision")?,field(&v,"id")?,&v["patch"]).map_err(math_error)?),
        _=>Err(ApiError::bad_request("invalid_json")),
    }
}
fn projections(root: &math_document::XmlNode) -> Value {
    fn visit(
        n: &math_document::XmlNode,
        map: &mut serde_json::Map<String, Value>,
        budget: &mut usize,
    ) {
        if matches!(
            n.local(),
            "r" | "e" | "num" | "den" | "sub" | "sup" | "deg" | "lim" | "fName"
        ) && *budget < 2 * 1024 * 1024
        {
            let tex = crate::math_formats::tex(n);
            *budget += tex.len();
            map.insert(n.id.clone(), json!(tex));
        }
        for c in n.children() {
            visit(c, map, budget);
        }
    }
    let mut map = serde_json::Map::new();
    visit(root, &mut map, &mut 0);
    Value::Object(map)
}

pub fn transact(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let v = input(req)?;
    let tx = serde_json::from_value(v["transaction"].clone())
        .map_err(|_| ApiError::bad_request("invalid_json"))?;
    let result = document_transaction::apply(field(&v, "content")?, &tx).map_err(math_error)?;
    ok_json(json!({"ok":true,"result":result}))
}
pub fn math_migrate(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let v = input(req)?;
    let source = field(&v, "content")?;
    if v["expected_revision"].as_str().is_some_and(|r| r != document_transaction::revision(source)) {
        return Err(math_error("transaction_conflict"));
    }
    let result = crate::math_migration::plan(source).map_err(math_error)?;
    ok_json(json!({"ok":true,"migration":result}))
}
pub fn math_parse(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let v = input(req)?;
    let source = field(&v, "source")?;
    let mut doc = math_document::parse(
        source,
        v["dialect"].as_str().unwrap_or("latex"),
        v["display"].as_bool().unwrap_or(false),
    )
    .map_err(math_error)?;
    let mut metadata_status = "absent";
    let mut identity = Value::Null;
    let mut extension_metadata = Value::Null;
    let mut preservation = Value::Null;
    if let Some(comment) = v["metadata"].as_str() {
        if comment.starts_with(crate::math_extension::PREFIX) {
            match crate::math_extension::resolve(source,comment,v["id"].as_str(),v["display"].as_bool().unwrap_or(false)) {
                Ok(resolved)=>{
                    doc=resolved.document;metadata_status="current";identity=json!(resolved.payload.id);
                    extension_metadata=json!(comment);
                    preservation=json!({"parts":resolved.payload.anchors.iter().map(|a|a.parts.len()+a.attributes.len()).sum::<usize>(),"editable":false,"preview":"standard_projection"});
                },
                Err("math_metadata_stale")=>metadata_status="stale",
                Err(_)=>metadata_status="invalid",
            }
        } else { match math_document::read_metadata(comment, source) {
            Ok((metadata, true)) => {
                doc = math_document::parse(
                    &metadata.omml,
                    "omml",
                    v["display"].as_bool().unwrap_or(false),
                )
                .map_err(math_error)?;
                metadata_status = "current";
                identity = json!(metadata.id);
            }
            Ok((_, false)) => metadata_status = "stale",
            Err(_) => metadata_status = "invalid",
        }}
    }
    let display = v["display"].as_bool().unwrap_or(false);
    let font_view=crate::omml::parse(&doc.root.xml()).map_err(|_|math_error("math_invalid_xml"))?;
    let _fonts=crate::math_layout::needs_managed_fonts(&font_view).then(||crate::publication_fonts::scope(&app.paths.data_dir,&app.paths.assets_dir));
    let font_warnings=crate::math_layout::font_warnings(&font_view);
    crate::math_layout::spacing_budget(&doc.root.xml(),display,math_preview_size(&v)?).map_err(math_error)?;
    let mathml = math_document::preview_constrained(&doc, display, math_preview_size(&v)?, math_preview_width(&v)?).map_err(math_error)?;
    let preview = mathml.output;
    let (vector_preview,layout_diagnostics)=math_vector_preview(&doc,display,math_preview_size(&v)?,math_preview_width(&v)?);
    if vector_preview.is_none()&&crate::math_spacing::contains_spacing(&doc.root){return Err(math_error("math_unsupported_extension"));}
    ok_json(
        json!({"ok":true,"document":doc,"preview":preview,"mathml_profile":mathml.profile,"mathml_warns":mathml.warnings,"vector_preview":vector_preview,"layout_diagnostics":layout_diagnostics,"latex":crate::math_formats::tex(&doc.root),"projections":projections(&doc.root),"visual_dsl":crate::math_dsl::visual_projections(&doc.root),"font_warnings":font_warnings,"metadata_status":metadata_status,"identity":identity,"metadata":extension_metadata,"extension_preservation":preservation}),
    )
}
pub fn math_patch(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let v = input(req)?;
    let op = serde_json::from_value(v["patch"].clone())
        .map_err(|_| ApiError::bad_request("invalid_json"))?;
    if let Some(comment)=v["metadata"].as_str().filter(|s|s.starts_with(crate::math_extension::PREFIX)) {
        crate::math_extension::resolve(field(&v,"source")?,comment,v["id"].as_str(),v["display"].as_bool().unwrap_or(false)).map_err(math_error)?;
    }
    let doc = math_document::patch(
        field(&v, "source")?,
        v["dialect"].as_str().unwrap_or("latex"),
        v["display"].as_bool().unwrap_or(false),
        field(&v, "expected_revision")?,
        &op,
    )
    .map_err(math_error)?;
    let extension_metadata=if let Some(comment)=v["metadata"].as_str().filter(|s|s.starts_with(crate::math_extension::PREFIX)){
        Some(crate::math_extension::rebind(field(&v,"source")?,comment,v["id"].as_str(),&doc,&op,v["display"].as_bool().unwrap_or(false)).map_err(math_error)?)
    }else{None};
    let identity=extension_metadata.as_ref().map(|comment|crate::math_extension::decode(comment).map(|p|p.id)).transpose().map_err(math_error)?;
    let font_view=crate::omml::parse(&doc.root.xml()).map_err(|_|math_error("math_invalid_xml"))?;
    let _fonts=crate::math_layout::needs_managed_fonts(&font_view).then(||crate::publication_fonts::scope(&app.paths.data_dir,&app.paths.assets_dir));
    let font_warnings=crate::math_layout::font_warnings(&font_view);
    crate::math_layout::spacing_budget(&doc.root.xml(),v["display"].as_bool().unwrap_or(false),math_preview_size(&v)?).map_err(math_error)?;
    let mathml = math_document::preview_constrained(&doc, v["display"].as_bool().unwrap_or(false), math_preview_size(&v)?, math_preview_width(&v)?)
        .map_err(math_error)?;
    let preview = mathml.output;
    let (vector_preview,layout_diagnostics)=math_vector_preview(&doc,v["display"].as_bool().unwrap_or(false),math_preview_size(&v)?,math_preview_width(&v)?);
    if vector_preview.is_none()&&crate::math_spacing::contains_spacing(&doc.root){return Err(math_error("math_unsupported_extension"));}
    ok_json(
        json!({"ok":true,"document":doc,"preview":preview,"mathml_profile":mathml.profile,"mathml_warns":mathml.warnings,"vector_preview":vector_preview,"layout_diagnostics":layout_diagnostics,"latex":crate::math_formats::tex(&doc.root),"projections":projections(&doc.root),"visual_dsl":crate::math_dsl::visual_projections(&doc.root),"font_warnings":font_warnings,"metadata":extension_metadata,"identity":identity,"metadata_status":if extension_metadata.is_some(){"current"}else{"absent"},"extension_preservation":extension_metadata.as_ref().map(|_|json!({"editable":false,"preview":"standard_projection"}))}),
    )
}
pub fn math_schema(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "GET" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    ok_json(json!({"ok":true,"schema":math_document::schema()}))
}
pub fn math_serialize(_app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    let v = input(req)?;
    let source = field(&v, "source")?;
    let dialect = v["dialect"].as_str().unwrap_or("latex");
    let display = v["display"].as_bool().unwrap_or(false);
    let doc = math_document::parse(source, dialect, display).map_err(math_error)?;
    if !doc.diagnostics.is_empty() {
        return Err(ApiError::new(422, "math_invalid_source")
            .noted_json("diagnostics", json!(doc.diagnostics)));
    }
    let mut extension_metadata=v["metadata"].as_str().filter(|s|s.starts_with(crate::math_extension::PREFIX)).map(String::from);
    let mut preservation=Value::Null;
    let xml = if let Some(comment)=&extension_metadata{
        let resolved=crate::math_extension::resolve(source,comment,v["id"].as_str(),display).map_err(math_error)?;
        preservation=json!({"parts":resolved.payload.anchors.iter().map(|a|a.parts.len()+a.attributes.len()).sum::<usize>(),"editable":false,"preview":"standard_projection"});resolved.xml
    }else{doc.root.xml()};
    if extension_metadata.is_some()&&matches!(v["target"].as_str(),Some("mathml"|"latex")){return Err(math_error("math_unsupported_extension"));}
    if matches!(v["target"].as_str().unwrap_or("omml"),"omml"|"latex")&&crate::math_spacing::contains_spacing(&doc.root){return Err(math_error("math_unsupported_extension"));}
    if v["target"].as_str()==Some("mathml"){crate::math_layout::spacing_budget(&xml,display,math_preview_size(&v)?).map_err(math_error)?;}
    let mathml=if v["target"].as_str()==Some("mathml"){
        let root=crate::omml::parse(&xml).map_err(|_|math_error("math_invalid_xml"))?;
        Some(crate::omml::to_mathml_constrained(&root,display,source,math_preview_size(&v)?,math_preview_width(&v)?).map_err(math_error)?)
    }else{None};
    let warns=mathml.as_ref().map(|p|p.warnings.clone()).unwrap_or_default();
    let output = match v["target"].as_str().unwrap_or("omml") {
        "dsl" => if dialect == "dsl" { source.into() } else {
            match crate::math_migration::readable(&xml){
                Ok(readable)=>readable,
                Err("math_unsupported_extension")=>{
                    let captured=crate::math_extension::capture(if dialect=="omml"{source}else{&xml},field(&v,"id")?,display).map_err(math_error)?;
                    preservation=json!({"parts":captured.preserved,"editable":false,"preview":"standard_projection"});extension_metadata=captured.metadata;captured.source
                },
                Err(code)=>return Err(math_error(code)),
            }
        },
        "omml" => xml,
        "mathml" => {
            mathml.as_ref().ok_or_else(||math_error("math_invalid_xml"))?.output.clone()
        }
        "latex" => {
            if dialect == "latex" {
                source.into()
            } else {
                crate::math_formats::tex_checked(&doc.root).map_err(math_error)?
            }
        }
        "metadata" => math_document::metadata(field(&v, "latex")?, &xml, field(&v, "id")?)
            .map_err(math_error)?,
        _ => return Err(math_error("math_unsupported_dialect")),
    };
    let warn_items=crate::api_codes::warn_items(&warns);
    ok_json(json!({"ok":true,"output":output,"revision":doc.revision,"mathml_profile":mathml.and_then(|p|p.profile),"warns":warns,"warn_items":warn_items,"metadata":extension_metadata,"extension_preservation":preservation}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equation_array_api_diagnostics_are_typed_projection_results_not_source_errors() {
        let source="<m:oMath><m:eqArr><m:eqArrPr><m:maxDist/><m:objDist/></m:eqArrPr><m:e><m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>a&amp;b&amp;x&amp;y</m:t></m:r></m:e><m:e><m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>x&amp;y&amp;b&amp;a</m:t></m:r></m:e></m:eqArr></m:oMath>";
        let doc=math_document::parse(source,"omml",true).unwrap();let before=doc.clone();
        let (svg,issues)=math_vector_preview(&doc,true,12.,Some(10.));assert!(svg.unwrap().contains("data-math-ink"));
        let value=serde_json::to_value(issues).unwrap();let issue=&value[0];
        assert_eq!(issue["code"],crate::api_codes::WARN_MATH_EQARR_OVERFLOW);
        assert!(issue["node_id"].as_str().unwrap().starts_with("rd-formula-token-omml-"));
        assert_eq!(issue["available_points"],10.);assert!(issue["required_points"].as_f64().unwrap()>10.);
        assert_eq!(doc.source,before.source);assert_eq!(doc.root,before.root);
        assert_eq!(serde_json::to_value(&doc.diagnostics).unwrap(),serde_json::to_value(&before.diagnostics).unwrap());
        let (_,wide)=math_vector_preview(&doc,true,12.,Some(96.));assert!(wide.is_empty());
    }
    #[test]
    fn math_preview_font_size_is_bounded_and_optional(){
        assert_eq!(math_preview_size(&json!({})).unwrap(),12.0);
        assert_eq!(math_preview_size(&json!({"font_size":18.0})).unwrap(),18.0);
        for value in [json!(-1),json!(0),json!(513),json!(null),json!("12")]{
            assert_eq!(math_preview_size(&json!({"font_size":value})).unwrap_err().status,400);
        }
    }
    #[test]fn math_preview_layout_width_is_bounded_optional_and_never_coerced(){
        assert_eq!(math_preview_width(&json!({})).unwrap(),None);
        assert_eq!(math_preview_width(&json!({"layout_width":300.0})).unwrap(),Some(300.0));
        for value in [json!(0),json!(-1),json!(20001),json!(null),json!("300"),json!([300])]{assert_eq!(math_preview_width(&json!({"layout_width":value})).unwrap_err().status,400);}
    }
    #[test]
    fn math_handlers_validate_methods_and_do_not_expose_internal_errors() {
        let request = Request {
            method: "GET".into(),
            path: "/api/math/parse".into(),
            query: Default::default(),
            headers: Default::default(),
            body: vec![],
        };
        assert_eq!(input(&request).unwrap_err().status, 405);
        assert_eq!(math_error("transaction_conflict").status, 409);
        assert_eq!(math_error("math_invalid_xml").status, 422);
    }
}
