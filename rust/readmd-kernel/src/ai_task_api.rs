//! One authorized task surface for desktop, MCP and editor extensions.
use crate::{ai_harness::{Budget,FrozenTemplate,TaskRecord,TaskRequest},server::{Request,Response},ApiError,ApiResult,App};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::sync::Arc;

fn error(code:&str)->ApiError{ApiError::new(if code=="review_not_authorized"{403}else if code.contains("persistence"){507}else if code.contains("budget"){413}else if code.contains("stale")||code.contains("conflict")||code=="cancelled"{409}else{422},code)}
fn source(app:&Arc<App>,document_id:&Value)->ApiResult<Value>{crate::review_api::snapshot(app,&json!({"document_id":document_id}))}
fn allowed(body:&Value,keys:&[&str])->ApiResult<()>{
    if body.as_object().is_none_or(|object|object.keys().any(|key|!keys.contains(&key.as_str()))){return Err(error("review_not_authorized"));}Ok(())
}
pub fn handle(app:&Arc<App>,req:&Request)->ApiResult<Response>{
    if req.method!="POST"{return Err(ApiError::new(405,"method_not_allowed"));}
    Ok(Response::json(&dispatch(app,&req.json()?)?))
}
pub(crate) fn dispatch(app:&Arc<App>,body:&Value)->ApiResult<Value>{
    match body["action"].as_str().unwrap_or(""){
        "tools"=>Ok(json!({"ok":true,"tools":crate::ai_harness::tool_registry()})),
        "list"=>{
            if !app.ai_tasks.has_data(){return Ok(json!({"ok":true,"tasks":[]}));}
            let tasks=app.ai_tasks.list().map_err(error)?.into_iter().map(|task|task_view(app,&task)).collect::<Vec<_>>();
            Ok(json!({"ok":true,"tasks":tasks}))
        },
        "start"=>start(app,body),
        "status"|"pause"|"resume"=>{
            if !app.ai_tasks.has_data(){return Err(error("review_not_found"));}
            allowed(body,&["action","task_id","after_event_id","call_id"])?;
            let id=body["task_id"].as_str().ok_or_else(||error("review_invalid_request"))?;
            let mut task=app.ai_tasks.get(id).map_err(error)?;
            if body["action"]=="pause"{task=app.ai_tasks.pause(id).map_err(error)?;}
            if body["action"]=="resume"&&!matches!(task.state.as_str(),"running"|"completed"){
                source(app,&json!(task.document_id))?;
                spawn(app,&task)?;task=app.ai_tasks.get(id).map_err(error)?;
            }
            response(app,&task,body["after_event_id"].as_u64().unwrap_or(0))
        },
        _=>Err(error("review_invalid_request")),
    }
}
fn start(app:&Arc<App>,body:&Value)->ApiResult<Value>{
    allowed(body,&["action","document_id","doc_id","generation","review_revision","request","call_id","provider","credential_id","model","skill_id","skill_variables","language","ranges","budget","stream"])?;
    if body.get("stream").is_some_and(|value|!value.is_boolean()){return Err(error("review_invalid_request"));}
    if body.get("document_id").is_some()&&body.get("doc_id").is_some()&&body["document_id"]!=body["doc_id"]{return Err(error("review_not_authorized"));}
    let document_id=body.get("document_id").or_else(||body.get("doc_id")).ok_or_else(||error("review_invalid_request"))?;
    let call=body["call_id"].as_str().filter(|s|crate::cancel::valid_id(s)).ok_or_else(||error("review_invalid_request"))?;
    let existing=if app.ai_tasks.has_data(){app.ai_tasks.lookup_start(call).map_err(error)?}else{None};
    let mut identity=body.clone();let object=identity.as_object_mut().unwrap();object.remove("action");object.remove("call_id");
    let caller_digest=caller_digest(&identity)?;
    if let Some(old)=&existing{
        if old.request.caller_digest!=caller_digest{return Err(error("review_call_conflict"));}
        return response(app,old,0);
    }
    let current=source(app,document_id)?;
    let mut initial=existing.as_ref().map(TaskRecord::initial_snapshot).unwrap_or(current);
    if initial["document_id"]!=*document_id{return Err(error("review_call_conflict"));}
    if initial["generation"]!=body["generation"]{return Err(error("review_stale_generation"));}
    if initial["review_revision"]!=body["review_revision"]{return Err(error("review_stale_revision"));}
    let ranges=crate::review_api::validate_ranges(&initial,body)?;
    if !ranges.is_empty(){initial["allowed_ranges"]=json!(ranges);}
    let instruction=body["request"].as_str().filter(|s|!s.trim().is_empty()).ok_or_else(||error("review_invalid_request"))?;
    let template=crate::server::review_task_template(app,body)?;
    let frozen=template.as_ref().map(|template|{
        let rendered=template.render(&initial,instruction)?;
        let fingerprint=template.identity()["fingerprint"].as_str().ok_or_else(||error("review_invalid_request"))?.to_string();
        Ok::<_,ApiError>(FrozenTemplate{skill_id:rendered.skill_id,fingerprint,rendered:rendered.rendered,
            language:body.get("language").or_else(||body.pointer("/skill_variables/language")).and_then(Value::as_str).unwrap_or("the document's language").into()})
    }).transpose()?;
    let route=crate::server::review_task_route(app,body)?;
    let connection=crate::server::review_task_connection(app,body,&route)?;
    let budget=body.get("budget").map(|b|serde_json::from_value::<Budget>(b.clone()).map_err(|_|error("review_ai_budget"))).transpose()?.unwrap_or_default();
    let request=TaskRequest{start_call_id:call.into(),document_id:initial["document_id"].as_str().ok_or_else(||error("review_invalid_request"))?.into(),instruction:instruction.into(),
        connection,allowed_ranges:(!ranges.is_empty()).then(||ranges.into_iter().map(|(from,to)|[from,to]).collect()),budget,template:frozen,
        stream:body["stream"].as_bool().unwrap_or(true),caller_digest};
    let task=app.ai_tasks.create(&initial,request).map_err(error)?;
    if existing.is_none(){spawn(app,&task)?;}
    let task=app.ai_tasks.get(&task.id).map_err(error)?;response(app,&task,0)
}
fn spawn(app:&Arc<App>,task:&TaskRecord)->ApiResult<()>{
    let connection=task.connection();
    let mut body=json!({"request":task.instruction(),"provider":connection.provider_id,"credential_id":connection.credential_id,"model":connection.model});
    let template=if let Some(saved)=task.template(){
        body["skill_id"]=json!(saved.skill_id);body["language"]=json!(saved.language);
        let current=crate::server::review_task_template(app,&body)?.ok_or_else(||error("review_conflict"))?;
        if current.identity()["fingerprint"]!=saved.fingerprint{return Err(error("review_conflict"));}
        Some(crate::ai_review::TaskTemplate{skill_id:saved.skill_id.clone(),rendered:saved.rendered.clone()})
    }else{None};
    let mut route=crate::server::review_task_route(app,&body)?;route.args.stream=task.request.stream;
    let permit=app.ai_tasks.start(&task.id).map_err(error)?;
    let app=app.clone();let id=task.id.clone();let on_failure=app.clone();
    std::thread::Builder::new().name("readmd-ai-task".into()).spawn(move||{
        let transport=crate::review_transport::ReviewTransport::new_owned(permit.checker());
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||app.ai_tasks.run(&permit,&app.review,&route,&transport,template.as_ref(),&|_|{})));
        if !matches!(result,Ok(Ok(_))){let _=app.ai_tasks.fail_start(&id,"review_persistence_failed");}
    }).map_err(|_|{let _=on_failure.ai_tasks.fail_start(&task.id,"review_persistence_failed");error("review_persistence_failed")})?;
    Ok(())
}
fn response(app:&Arc<App>,task:&TaskRecord,after:u64)->ApiResult<Value>{
    let mut result=json!({"ok":true,"task":task_view(app,task),"events":app.ai_tasks.events(&task.id,after).map_err(error)?});
    if let Ok(snapshot)=source(app,&json!(task.document_id)){result["snapshot"]=snapshot;}Ok(result)
}
fn task_view(app:&App,task:&TaskRecord)->Value{
    let mut value=task.public_view();value["owned_here"]=json!(app.ai_tasks.owns_task(task));value
}
fn caller_digest(value:&Value)->ApiResult<String>{
    fn canonical(value:&Value)->Value{
        match value{
            Value::Object(object)=>{let sorted=object.iter().map(|(key,value)|(key.clone(),canonical(value))).collect::<std::collections::BTreeMap<_,_>>();serde_json::to_value(sorted).unwrap_or(Value::Null)},
            Value::Array(items)=>Value::Array(items.iter().map(canonical).collect()),
            _=>value.clone(),
        }
    }
    Ok(format!("{:x}",Sha256::digest(serde_json::to_vec(&canonical(value)).map_err(|_|error("review_invalid_request"))?)))
}

#[cfg(test)]
#[path="ai_task_api_tests.rs"]
mod tests;
