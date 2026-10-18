//! Reproducible attribution for the exact locked CSL archive.
fn main() {
    let styles = hayagriva::archive::ArchivedStyle::all()
        .iter()
        .map(|style| {
            let info = match style.get() {
                hayagriva::citationberg::Style::Independent(s) => s.info,
                hayagriva::citationberg::Style::Dependent(s) => s.info,
            };
            serde_json::json!({"names":style.names(),"info":info})
        })
        .collect::<Vec<_>>();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"library":"Hayagriva 0.10.1","archiveLicense":"CC-BY-SA-3.0","modified":false,"styles":styles})).unwrap());
}
