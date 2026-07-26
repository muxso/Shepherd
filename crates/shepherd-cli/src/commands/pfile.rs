use crate::client::*;
use base64::Engine;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PfileCmd {
    /// List a project's files.
    List {
        #[arg(long)]
        project: String,
    },
    /// Upload a local file (base64-encoded and stored).
    Upload {
        #[arg(long)]
        project: String,
        /// Local file path to upload.
        #[arg(long)]
        file: String,
        /// File name (defaults to the local file name).
        #[arg(long)]
        name: Option<String>,
        /// File format / extension hint (e.g. png, pdf, md).
        #[arg(long, default_value = "")]
        format: String,
        /// Attach to a module id.
        #[arg(long = "module-id")]
        module_id: Option<String>,
    },
    /// Delete a file by id.
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Move a file into (or out of) a module.
    Move {
        #[arg(long)]
        id: String,
        /// Target module id (omit with --unset to move to uncategorized).
        #[arg(long = "module-id")]
        module_id: Option<String>,
        /// Move the file out of its module.
        #[arg(long, default_value_t = false)]
        unset: bool,
    },
    /// Download a file (decodes contentBase64 to --out).
    Download {
        #[arg(long)]
        id: String,
        #[arg(long)]
        out: String,
    },
    /// Fetch the raw decoded bytes of a file to --out (images/docs).
    Raw {
        #[arg(long)]
        id: String,
        #[arg(long)]
        out: String,
    },
}

pub fn run(cmd: PfileCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        PfileCmd::List { project } => {
            pretty(&c.get(&format!("/api/project-file?projectId={project}"), true)?)
        }
        PfileCmd::Upload { project, file, name, format, module_id } => {
            let bytes = std::fs::read(&file)?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let fname = name.unwrap_or_else(|| {
                std::path::Path::new(&file)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into())
            });
            let mut body = json!({
                "projectId": project,
                "name": fname,
                "fileFormat": format,
                "sizeBytes": bytes.len() as i64,
                "contentBase64": b64,
            });
            if let Some(m) = module_id {
                body["moduleId"] = json!(m);
            }
            let v = c.post("/api/project-file", body, true)?;
            println!(" 已上传 {}", v.get("id").and_then(|x| x.as_str()).unwrap_or(""));
            pretty(&v);
        }
        PfileCmd::Delete { id } => {
            c.delete(&format!("/api/project-file/{id}"), true)?;
            println!(" 已删除文件 {id}");
        }
        PfileCmd::Move { id, module_id, unset } => {
            let mid = if unset { None } else { module_id };
            c.put(&format!("/api/project-file/{id}/module"), json!({ "moduleId": mid }), true)?;
            println!(" 文件 {id} 已{}", if mid.is_some() { "移入模块" } else { "移出到未归类" });
        }
        PfileCmd::Download { id, out } => {
            let v = c.get(&format!("/api/project-file/{id}/download"), true)?;
            let b64 =
                v.get("contentBase64").and_then(|x| x.as_str()).ok_or("响应缺少 contentBase64")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|e| format!("base64 解码失败:{e}"))?;
            std::fs::write(&out, &bytes)?;
            println!(" 已下载 {} 字节 → {out}", bytes.len());
        }
        PfileCmd::Raw { id, out } => {
            let bytes = c.get_bytes(&format!("/api/project-file/{id}/raw"), true)?;
            std::fs::write(&out, &bytes)?;
            println!(" 已写出 {out} ({} 字节)", bytes.len());
        }
    };
    Ok(())
}
