//! Native RFC 5322/MIME and Outlook MSG conversion, with bounded attachments.
use base64::Engine;
use std::collections::BTreeMap;
use std::path::{Path,PathBuf};

type Headers = BTreeMap<String,String>;
const LIMIT: usize = 64 * 1024 * 1024;

fn split_message(bytes: &[u8]) -> (Headers,&[u8]) {
    let (end,skip) = bytes.windows(4).position(|p|p==b"\r\n\r\n").map(|i|(i,4))
        .or_else(||bytes.windows(2).position(|p|p==b"\n\n").map(|i|(i,2))).unwrap_or((0,0));
    let mut headers=Headers::new(); let mut key=String::new();
    for line in String::from_utf8_lossy(&bytes[..end]).lines() {
        if line.starts_with([' ','\t']) && !key.is_empty() {
            if let Some(value)=headers.get_mut(&key) { value.push(' ');value.push_str(line.trim()); }
        } else if let Some((k,v))=line.split_once(':') {
            key=k.to_ascii_lowercase();headers.insert(key.clone(),v.trim().into());
        }
    }
    (headers,&bytes[end+skip..])
}

fn parameter(value: &str, name: &str) -> Option<String> {
    let re=regex::Regex::new(&format!(r#"(?i)(?:^|;)\s*{}\s*=\s*(?:"([^"]*)"|([^;\s]*))"#,regex::escape(name))).ok()?;
    let c=re.captures(value)?; Some(c.get(1).or_else(||c.get(2))?.as_str().to_string())
}

fn quoted_printable(bytes: &[u8], header: bool) -> Vec<u8> {
    let mut out=Vec::new();let mut i=0;
    while i<bytes.len() {
        if bytes[i]==b'=' {
            if bytes.get(i+1)==Some(&b'\n') { i+=2;continue; }
            if bytes.get(i+1..i+3)==Some(b"\r\n") { i+=3;continue; }
            if let Some(pair)=bytes.get(i+1..i+3) {
                if let Ok(s)=std::str::from_utf8(pair) { if let Ok(b)=u8::from_str_radix(s,16) {out.push(b);i+=3;continue;} }
            }
        }
        out.push(if header && bytes[i]==b'_' {b' '} else {bytes[i]});i+=1;
    }
    out
}

fn decode(bytes: &[u8], charset: &str) -> String {
    let enc=encoding_rs::Encoding::for_label(charset.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    enc.decode(bytes).0.trim_end_matches('\0').to_string()
}

fn encoded_header(value: &str) -> String {
    let re=regex::Regex::new(r"=\?([^?]+)\?([bBqQ])\?([^?]*)\?=").expect("MIME header regex");
    let mut out=String::new();let mut cursor=0;let mut encoded=false;
    for c in re.captures_iter(value) {
        let m=c.get(0).unwrap();let gap=&value[cursor..m.start()];
        if !encoded || !gap.trim().is_empty() {out.push_str(gap);}
        let bytes=if c[2].eq_ignore_ascii_case("b") {base64::engine::general_purpose::STANDARD.decode(&c[3]).unwrap_or_else(|_|c[3].as_bytes().to_vec())} else {quoted_printable(c[3].as_bytes(),true)};
        out.push_str(&decode(&bytes,&c[1]));cursor=m.end();encoded=true;
    }
    out.push_str(&value[cursor..]);out
}

struct Attachment { name:String, bytes:Vec<u8>, cid:String, image:bool }
#[derive(Default)]
struct Parts { plain:Vec<String>,html:Vec<String>,attachments:Vec<Attachment>,count:usize }

fn mime(bytes: &[u8], depth:usize, parts:&mut Parts) -> Result<(),String> {
    parts.count+=1;
    if depth>8 || parts.count>256 {return Err("mime_part_limit_exceeded".into());}
    let (h,body)=split_message(bytes);
    let content=h.get("content-type").map(String::as_str).unwrap_or("text/plain");
    if content.to_ascii_lowercase().starts_with("multipart/") {
        let boundary=parameter(content,"boundary").filter(|s|!s.is_empty() && s.len()<=200).ok_or("mime_boundary_missing")?;
        let marker=format!("--{boundary}");let closing=format!("--{boundary}--");
        let mut start=None;let mut offset=0;
        for line in body.split_inclusive(|b|*b==b'\n') {
            let clean=String::from_utf8_lossy(line);let clean=clean.trim_end_matches(['\r','\n',' ','\t']);
            if clean==marker || clean==closing {
                if let Some(begin)=start.take() {mime(&body[begin..offset],depth+1,parts)?;}
                if clean==closing {return Ok(());}start=Some(offset+line.len());
            }
            offset+=line.len();
        }
        if let Some(begin)=start {mime(&body[begin..],depth+1,parts)?;}
        return Ok(());
    }
    let transfer=h.get("content-transfer-encoding").map(|s|s.trim().to_ascii_lowercase()).unwrap_or_default();
    let data=match transfer.as_str() {
        "base64"=>base64::engine::general_purpose::STANDARD.decode(body.iter().copied().filter(|b|!b.is_ascii_whitespace()).collect::<Vec<_>>()).map_err(|_|"invalid_mime_base64")?,
        "quoted-printable"=>quoted_printable(body,false),_=>body.to_vec(),
    };
    let disposition=h.get("content-disposition").map(String::as_str).unwrap_or("");
    let name=parameter(disposition,"filename*").map(|s| {
        let (charset,encoded)=s.split_once("''").unwrap_or(("utf-8",&s));decode(&percent_encoding::percent_decode_str(encoded).collect::<Vec<_>>(),charset)
    }).or_else(||parameter(disposition,"filename")).or_else(||parameter(content,"name")).map(|s|encoded_header(&s));
    if name.is_some() || disposition.to_ascii_lowercase().starts_with("attachment") || content.to_ascii_lowercase().starts_with("image/") {
        parts.attachments.push(Attachment{name:name.unwrap_or_else(||format!("attachment-{}.bin",parts.attachments.len()+1)),bytes:data,cid:h.get("content-id").map(|s|s.trim_matches(['<','>']).to_string()).unwrap_or_default(),image:content.to_ascii_lowercase().starts_with("image/")});
    } else if content.to_ascii_lowercase().starts_with("text/") {
        let text=decode(&data,&parameter(content,"charset").unwrap_or_else(||"utf-8".into()));
        if content.to_ascii_lowercase().starts_with("text/html") {parts.html.push(text);} else {parts.plain.push(text);}
    } else if content.to_ascii_lowercase().starts_with("message/rfc822") {mime(&data,depth+1,parts)?;}
    Ok(())
}

fn save_attachments(path:&Path, attachments:Vec<Attachment>) -> Result<(Vec<String>,Vec<(String,String)>),String> {
    let parent=path.parent().unwrap_or(Path::new("."));
    let asset_name=format!("{}.assets",path.file_stem().unwrap_or_default().to_string_lossy());
    let assets=parent.join(&asset_name);let mut links=Vec::new();let mut cids=Vec::new();
    if attachments.is_empty() {return Ok((links,cids));}
    std::fs::create_dir_all(&assets).map_err(|e|e.to_string())?;
    if !assets.canonicalize().map_err(|e|e.to_string())?.starts_with(parent.canonicalize().map_err(|e|e.to_string())?) {return Err("mail_assets_outside_directory".into());}
    for (index,a) in attachments.into_iter().enumerate() {
        let clean:String=a.name.rsplit(['/', '\\']).next().unwrap_or("attachment.bin").chars().filter(|c|!c.is_control() && !"<>:\"|?*".contains(*c)).take(120).collect();
        let clean=clean.trim_matches(['.',' ']);let clean=if clean.is_empty(){"attachment.bin"}else{clean};
        let name=format!("mail-{}-{}-{clean}",index+1,&uuid::Uuid::new_v4().simple().to_string()[..8]);
        let target=assets.join(&name);
        crate::content::write_bytes_atomic(&target,&a.bytes).map_err(|e|e.to_string())?;
        let relative=format!("{asset_name}/{name}").replace('\\',"/");
        let url=percent_encoding::utf8_percent_encode(&relative,percent_encoding::NON_ALPHANUMERIC).to_string().replace("%2F","/");
        let label=crate::convert::md_cell(clean).replace('[',"\\[").replace(']',"\\]");
        links.push(format!("{}[{label}]({url})",if a.image{"!"}else{""}));
        if !a.cid.is_empty() {cids.push((a.cid,url));}
    }
    Ok((links,cids))
}

fn finish(path:&Path,headers:Headers,parts:Parts) -> Result<String,String> {
    let (links,cids)=save_attachments(path,parts.attachments)?;
    let title=encoded_header(headers.get("subject").map(String::as_str).unwrap_or("Email"));
    let mut out=format!("# {}\n\n",title.replace(['\r','\n']," "));
    for (key,label) in [("from","From"),("to","To"),("cc","Cc"),("date","Date")] {
        if let Some(value)=headers.get(key).filter(|v|!v.trim().is_empty()) {out.push_str(&format!("**{label}:** {}\n\n",encoded_header(value).replace(['\r','\n']," ")));}
    }
    if !parts.html.is_empty() {
        let mut html=parts.html.join("\n");for (cid,url) in cids {html=html.replace(&format!("cid:{cid}"),&url);}
        out.push_str(&crate::headless_renderer::html_to_markdown(&html));
    } else {out.push_str(&parts.plain.join("\n\n"));}
    if !links.is_empty() {out.push_str("\n\n## Attachments\n\n");out.push_str(&links.join("\n\n"));}
    if parts.plain.is_empty() && parts.html.is_empty() && links.is_empty() {return Err("mail_body_empty".into());}
    out.push('\n');Ok(out)
}

pub fn eml_to_md(path:&str) -> Result<String,String> {
    let bytes=read(path)?;let (headers,_)=split_message(&bytes);let mut parts=Parts::default();
    mime(&bytes,0,&mut parts)?;finish(Path::new(path),headers,parts)
}

fn read(path:&str) -> Result<Vec<u8>,String> {
    if std::fs::metadata(path).map_err(|e|e.to_string())?.len()>LIMIT as u64 {return Err("mail_too_large".into());}
    std::fs::read(path).map_err(|e|e.to_string())
}

fn msg_string(cfb:&crate::convert::CfbReader<'_>,prefix:&str,tag:&str) -> Option<String> {
    if let Some(data)=cfb.get_path_stream(&format!("{prefix}__substg1.0_{tag}001f")) {
        let units:Vec<_>=data.chunks_exact(2).map(|s|u16::from_le_bytes([s[0],s[1]])).collect();return Some(String::from_utf16_lossy(&units).trim_end_matches('\0').into());
    }
    cfb.get_path_stream(&format!("{prefix}__substg1.0_{tag}001e")).map(|b|decode(&b,&msg_charset(cfb,prefix)))
}

fn msg_charset(cfb:&crate::convert::CfbReader<'_>,prefix:&str)->String {
    let properties=cfb.get_path_stream(&format!("{prefix}__properties_version1.0")).unwrap_or_default();
    for p in properties.get(if prefix.is_empty(){32}else{24}..).unwrap_or_default().chunks_exact(16) {
        let tag=u32::from_le_bytes(p[..4].try_into().unwrap());
        if tag==0x3fde0003 || tag==0x3ffd0003 {
            return match u32::from_le_bytes(p[8..12].try_into().unwrap()) {
                65001=>"utf-8".into(),936=>"gb18030".into(),950=>"big5".into(),932=>"shift_jis".into(),949=>"euc-kr".into(),
                cp @ 1250..=1258=>format!("windows-{cp}"),_=>"windows-1252".into(),
            };
        }
    }
    "windows-1252".into()
}

/// MS-OXRTFCP: a bounded 4 KiB LZFu dictionary, or uncompressed MELA data.
fn msg_rtf(bytes:&[u8])->Result<Vec<u8>,String> {
    if bytes.len()<16 {return Err("invalid_msg_rtf".into());}
    let size=u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let packed=u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    if size>LIMIT || packed<12 || packed.checked_add(4).is_none_or(|n|n>bytes.len()) {return Err("invalid_msg_rtf".into());}
    let input=&bytes[16..packed+4];
    if &bytes[8..12]==b"MELA" {return input.get(..size).map(|b|b.to_vec()).ok_or_else(||"invalid_msg_rtf".into());}
    if &bytes[8..12]!=b"LZFu" {return Err("invalid_msg_rtf".into());}
    let initial=b"{\\rtf1\\ansi\\mac\\deff0\\deftab720{\\fonttbl;}{\\f0\\fnil \\froman \\fswiss \\fmodern \\fscript \\fdecor MS Sans SerifSymbolArialTimes New RomanCourier{\\colortbl\\red0\\green0\\blue0\r\n\\par \\pard\\plain\\f0\\fs20\\b\\i\\u\\tab\\tx";
    let mut dictionary=[0u8;4096];dictionary[..initial.len()].copy_from_slice(initial);
    let mut write=initial.len();let mut cursor=0;let mut output=Vec::with_capacity(size.min(1024*1024));
    'runs: while output.len()<size && cursor<input.len() {
        let flags=input[cursor];cursor+=1;
        for bit in 0..8 {
            if output.len()>=size {break 'runs;}
            if flags & (1<<bit)!=0 {
                let pair=input.get(cursor..cursor+2).ok_or("invalid_msg_rtf")?;cursor+=2;
                let token=u16::from_be_bytes([pair[0],pair[1]]);let offset=(token>>4) as usize;let length=(token&15) as usize+2;
                if offset==write {break 'runs;}
                for i in 0..length {if output.len()>=size {break;}let b=dictionary[(offset+i)%4096];output.push(b);dictionary[write]=b;write=(write+1)%4096;}
            }else{
                let b=*input.get(cursor).ok_or("invalid_msg_rtf")?;cursor+=1;output.push(b);dictionary[write]=b;write=(write+1)%4096;
            }
        }
    }
    if output.len()!=size {return Err("invalid_msg_rtf".into());}Ok(output)
}

fn msg_parts(cfb:&crate::convert::CfbReader<'_>,prefix:&str,depth:usize) -> Result<(Headers,Parts),String> {
    if depth>8 {return Err("msg_depth_exceeded".into());}
    let mut headers=Headers::new();let mut parts=Parts::default();
    for (key,tag) in [("subject","0037"),("from","0c1a"),("to","0e04"),("cc","0e03")] {if let Some(value)=msg_string(cfb,prefix,tag) {headers.insert(key.into(),value);}}
    if let Some(h)=msg_string(cfb,prefix,"007d") {let (transport,_)=split_message(format!("{h}\r\n\r\n").as_bytes());for (k,v) in transport {headers.entry(k).or_insert(v);}}
    if let Some(data)=cfb.get_path_stream(&format!("{prefix}__substg1.0_10130102")) {
        parts.html.push(if std::str::from_utf8(&data).is_ok(){decode(&data,"utf-8")}else{decode(&data,&msg_charset(cfb,prefix))});
    }
    if let Some(text)=msg_string(cfb,prefix,"1000") {parts.plain.push(text);}
    if parts.html.is_empty() && parts.plain.iter().all(|s|s.trim().is_empty()) {
        if let Some(bytes)=cfb.get_path_stream(&format!("{prefix}__substg1.0_10090102")) {
            let raw=msg_rtf(&bytes)?;parts.plain.push(crate::convert::extract_rtf_text(&String::from_utf8_lossy(&raw)));
        }
    }
    let paths=cfb.stream_paths();
    let mut storages=std::collections::BTreeSet::new();
    for path in &paths {
        if let Some(rest)=path.strip_prefix(prefix) {
            if let Some((storage,_))=rest.split_once('/') {if storage.starts_with("__attach_version1.0_") {storages.insert(format!("{prefix}{storage}/"));}}
        }
    }
    for storage in storages {
        let name=msg_string(cfb,&storage,"3707").or_else(||msg_string(cfb,&storage,"3704")).unwrap_or_else(||"attachment.bin".into());
        if let Some(bytes)=cfb.get_path_stream(&format!("{storage}__substg1.0_37010102")) {
            let cid=msg_string(cfb,&storage,"3712").unwrap_or_default();
            let image=msg_string(cfb,&storage,"370e").is_some_and(|s|s.starts_with("image/"));
            parts.attachments.push(Attachment{name,bytes,cid,image});
        } else {
            let embedded=format!("{storage}__substg1.0_3701000d/");
            if paths.iter().any(|p|p.starts_with(&embedded)) {
                let (h,p)=msg_parts(cfb,&embedded,depth+1)?;
                parts.plain.push(format!("## Embedded message\n\n{}\n\n{}",h.get("subject").cloned().unwrap_or_default(),if p.html.is_empty(){p.plain.join("\n")}else{crate::headless_renderer::html_to_markdown(&p.html.join("\n"))}));
                parts.attachments.extend(p.attachments);
            }
        }
    }
    Ok((headers,parts))
}

pub fn msg_to_md(path:&str) -> Result<String,String> {
    let bytes=read(path)?;let cfb=crate::convert::CfbReader::parse(&bytes).ok_or("invalid_msg_container")?;
    let (headers,parts)=msg_parts(&cfb,"",0)?;finish(&PathBuf::from(path),headers,parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(name:&str,kind:u8,right:u32,child:u32,start:u32,size:u64)->[u8;128] {
        let mut e=[0u8;128];let units:Vec<_>=name.encode_utf16().collect();
        for (i,u) in units.iter().enumerate(){e[i*2..i*2+2].copy_from_slice(&u.to_le_bytes());}
        e[64..66].copy_from_slice(&((units.len()+1) as u16*2).to_le_bytes());e[66]=kind;e[67]=1;
        e[68..72].copy_from_slice(&u32::MAX.to_le_bytes());e[72..76].copy_from_slice(&right.to_le_bytes());e[76..80].copy_from_slice(&child.to_le_bytes());e[116..120].copy_from_slice(&start.to_le_bytes());e[120..128].copy_from_slice(&size.to_le_bytes());e
    }
    #[test]
    fn msg_mini_streams_keep_root_body_and_attachment_storages_distinct() {
        let mut file=vec![0u8;3072];file[..8].copy_from_slice(&[0xd0,0xcf,0x11,0xe0,0xa1,0xb1,0x1a,0xe1]);
        file[24..26].copy_from_slice(&0x3eu16.to_le_bytes());file[26..28].copy_from_slice(&3u16.to_le_bytes());file[28..30].copy_from_slice(&0xfffeu16.to_le_bytes());file[30..32].copy_from_slice(&9u16.to_le_bytes());file[32..34].copy_from_slice(&6u16.to_le_bytes());
        file[44..48].copy_from_slice(&1u32.to_le_bytes());file[48..52].copy_from_slice(&1u32.to_le_bytes());file[56..60].copy_from_slice(&4096u32.to_le_bytes());file[60..64].copy_from_slice(&3u32.to_le_bytes());file[64..68].copy_from_slice(&1u32.to_le_bytes());file[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
        for offset in (76..512).step_by(4) {file[offset..offset+4].copy_from_slice(&u32::MAX.to_le_bytes());}file[76..80].copy_from_slice(&0u32.to_le_bytes());
        for offset in (512..1024).step_by(4){file[offset..offset+4].copy_from_slice(&u32::MAX.to_le_bytes());}
        for (i,n) in [0xfffffffdu32,2,0xfffffffe,0xfffffffe,0xfffffffe].iter().enumerate(){file[512+i*4..516+i*4].copy_from_slice(&n.to_le_bytes());}
        let unicode=|s:&str|s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect::<Vec<_>>();
        let streams=[unicode("中文邮件"),unicode("Root message body"),unicode("Reader"),unicode("../../memo.txt"),b"attachment bytes".to_vec(),unicode("Nested body")];
        let entries=[directory("Root Entry",5,u32::MAX,1,4,512),directory("__substg1.0_0037001F",2,2,u32::MAX,0,streams[0].len() as u64),directory("__substg1.0_1000001F",2,3,u32::MAX,1,streams[1].len() as u64),directory("__substg1.0_0C1A001F",2,4,u32::MAX,2,streams[2].len() as u64),directory("__attach_version1.0_#00000000",1,u32::MAX,5,0,0),directory("__substg1.0_3707001F",2,6,u32::MAX,3,streams[3].len() as u64),directory("__substg1.0_37010102",2,7,u32::MAX,4,streams[4].len() as u64),directory("__substg1.0_1000001F",2,u32::MAX,u32::MAX,5,streams[5].len() as u64)];
        for (i,e) in entries.iter().enumerate(){file[1024+i*128..1152+i*128].copy_from_slice(e);}
        for offset in (2048..2560).step_by(4){file[offset..offset+4].copy_from_slice(&u32::MAX.to_le_bytes());}
        for (i,stream) in streams.iter().enumerate(){file[2048+i*4..2052+i*4].copy_from_slice(&0xfffffffeu32.to_le_bytes());file[2560+i*64..2560+i*64+stream.len()].copy_from_slice(stream);}
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("message.msg");std::fs::write(&path,file).unwrap();
        let md=msg_to_md(path.to_str().unwrap()).unwrap();assert!(md.contains("中文邮件"));assert!(md.contains("Root message body"));assert!(!md.contains("Nested body"));assert!(md.contains("[memo.txt]"));
        let attachment=std::fs::read_dir(dir.path().join("message.assets")).unwrap().next().unwrap().unwrap();assert_eq!(std::fs::read(attachment.path()).unwrap(),b"attachment bytes");
    }
    #[test]
    fn compressed_msg_rtf_matches_ms_oxrtfcp_repeating_dictionary_example() {
        let packed=[0x41,0,4,0x20,0x57,0x58,0x59,0x5a,0x0d,0x6e,0x7d];
        let expected=b"{\\rtf1 WXYZWXYZWXYZWXYZWXYZ}";
        let mut bytes=Vec::new();bytes.extend_from_slice(&((packed.len()+12) as u32).to_le_bytes());bytes.extend_from_slice(&(expected.len() as u32).to_le_bytes());bytes.extend_from_slice(b"LZFu");bytes.extend_from_slice(&0u32.to_le_bytes());bytes.extend_from_slice(&packed);
        assert_eq!(msg_rtf(&bytes).unwrap(),expected);assert!(msg_rtf(&bytes[..8]).is_err());
    }
    #[test]
    fn mime_alternative_encoded_headers_and_safe_attachment() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("mail.eml");
        std::fs::write(&path,b"Subject: =?UTF-8?B?5Lit5paH?=\r\nFrom: Reader <reader@example.test>\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nHello=20Reader\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"../../memo.txt\"\r\nContent-Transfer-Encoding: base64\r\n\r\nbWVtbw==\r\n--x--\r\n").unwrap();
        let md=eml_to_md(path.to_str().unwrap()).unwrap();assert!(md.contains("# 中文"));assert!(md.contains("Hello Reader"));assert!(md.contains("[memo.txt]"));
        let entries:Vec<_>=std::fs::read_dir(dir.path().join("mail.assets")).unwrap().flatten().collect();assert_eq!(entries.len(),1);assert_eq!(std::fs::read(entries[0].path()).unwrap(),b"memo");assert!(!dir.path().parent().unwrap().join("memo.txt").exists());
    }
    #[test]
    fn mime_html_and_invalid_body() {
        let mut parts=Parts::default();mime(b"Content-Type: multipart/alternative; boundary=b\n\n--b\nContent-Type: text/plain\n\nFallback\n--b\nContent-Type: text/html\n\n<p>Actual <strong>body</strong></p>\n--b--",0,&mut parts).unwrap();
        let dir=tempfile::tempdir().unwrap();let md=finish(&dir.path().join("mail.eml"),Headers::new(),parts).unwrap();assert!(md.contains("Actual **body**"));assert!(!md.contains("Fallback"));
        assert!(mime(b"Content-Type: text/plain\nContent-Transfer-Encoding: base64\n\n???",0,&mut Parts::default()).is_err());
    }
}
