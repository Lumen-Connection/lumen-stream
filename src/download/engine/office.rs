use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::config::settings::ConvertEngine;

use super::fs_utils::find_named;
use super::pdf::render_text_pdf;
use super::DownloadEngine;

impl DownloadEngine {
    pub(super) async fn office_convert(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
        engine: ConvertEngine,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        // TXT é sempre extração nativa (rápida e sem dependência).
        // Para PDF, respeita a escolha do usuário; Auto pega o melhor disponível.
        // md e txt são sempre nativos: LibreOffice/MS Office não agregam valor.
        let chosen = if format == "txt" || format == "md" {
            ConvertEngine::Rust
        } else {
            match engine {
                ConvertEngine::Auto => auto_pick_engine(),
                other => other,
            }
        };

        match chosen {
            ConvertEngine::MsOffice => self.office_via_msoffice(input, out, format).await,
            ConvertEngine::LibreOffice => self.office_via_libreoffice(input, out, format).await,
            _ => self.office_via_native(input, out, format).await,
        }
    }

    async fn office_via_native(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input = input.to_path_buf();
        let out_path = out.to_path_buf();
        let out_ret = out_path.clone();
        let format = format.to_string();
        let title = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Documento".to_string());

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let text = office_extract_text(&input)?;
            match format.as_str() {
                "txt" => std::fs::write(&out_path, text).map_err(|e| e.to_string()),
                "md" => {
                    let md = super::markdown::document_to_markdown(&input)?;
                    std::fs::write(&out_path, md).map_err(|e| e.to_string())
                }
                "pdf" => render_text_pdf(&text, &out_path, &title),
                other => Err(format!(
                    "Conversão nativa para \"{}\" não suportada. Use PDF, TXT ou MD.",
                    other
                )),
            }
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(out_ret)
    }

    async fn office_via_libreoffice(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let soffice = libreoffice_path()
            .ok_or("LibreOffice não encontrado. Instale-o ou escolha outro motor.")?;
        let outdir = out.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let input_stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "doc".to_string());

        let mut cmd = tokio::process::Command::new(&soffice);
        cmd.arg("--headless")
            .arg("--convert-to")
            .arg(format)
            .arg("--outdir")
            .arg(&outdir)
            .arg(input);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let result = cmd.output().await.map_err(|e| e.to_string())?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("LibreOffice falhou: {}", stderr.trim()).into());
        }

        let produced = outdir.join(format!("{}.{}", input_stem, format));
        if produced.exists() {
            if produced != out {
                let _ = std::fs::rename(&produced, out);
                if out.exists() {
                    return Ok(out.to_path_buf());
                }
                return Ok(produced);
            }
            return Ok(produced);
        }
        Err("Arquivo convertido não encontrado.".into())
    }

    async fn office_via_msoffice(
        &self,
        input: &Path,
        out: &Path,
        format: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        if format != "pdf" {
            return Err("MS Office só é usado aqui para gerar PDF.".into());
        }
        #[cfg(not(windows))]
        {
            let _ = (input, out);
            return Err("MS Office só está disponível no Windows.".into());
        }
        #[cfg(windows)]
        {
            let input = input.to_path_buf();
            let out_path = out.to_path_buf();
            let out_ret = out_path.clone();
            tokio::task::spawn_blocking(move || msoffice_to_pdf(&input, &out_path))
                .await
                .map_err(|e| e.to_string())??;
            Ok(out_ret)
        }
    }
}

pub struct EngineStatus {
    pub msoffice: bool,
    pub msoffice_detail: String,
    pub libreoffice: bool,
}

static ENGINE_STATUS: OnceLock<EngineStatus> = OnceLock::new();

/// Detecta (uma vez por sessão) os motores externos disponíveis.
pub fn engine_status() -> &'static EngineStatus {
    ENGINE_STATUS.get_or_init(|| {
        let mut apps: Vec<&str> = Vec::new();
        if msoffice_exe("WINWORD.EXE").is_some() {
            apps.push("Word");
        }
        if msoffice_exe("EXCEL.EXE").is_some() {
            apps.push("Excel");
        }
        if msoffice_exe("POWERPNT.EXE").is_some() {
            apps.push("PowerPoint");
        }
        EngineStatus {
            msoffice: !apps.is_empty(),
            msoffice_detail: apps.join(", "),
            libreoffice: libreoffice_path().is_some(),
        }
    })
}

/// No modo Automático, escolhe o motor de maior fidelidade disponível.
fn auto_pick_engine() -> ConvertEngine {
    let st = engine_status();
    if st.msoffice {
        ConvertEngine::MsOffice
    } else if st.libreoffice {
        ConvertEngine::LibreOffice
    } else {
        ConvertEngine::Rust
    }
}

pub fn libreoffice_path() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/usr/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

fn msoffice_exe(name: &str) -> Option<PathBuf> {
    let roots = [
        r"C:\Program Files\Microsoft Office",
        r"C:\Program Files (x86)\Microsoft Office",
    ];
    for r in roots {
        let root = Path::new(r);
        if root.exists() {
            if let Some(found) = find_named(root, name, 4) {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(windows)]
fn msoffice_to_pdf(input: &Path, out: &Path) -> Result<(), String> {
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let inp = input.to_string_lossy().replace('\'', "''");
    let outp = out.to_string_lossy().replace('\'', "''");

    let script = match ext.as_str() {
        "doc" | "docx" | "odt" | "rtf" | "txt" => format!(
            "$w = New-Object -ComObject Word.Application; $w.Visible = $false; \
             try {{ $d = $w.Documents.Open('{inp}'); $d.SaveAs([ref]'{outp}', [ref]17); $d.Close($false) }} \
             finally {{ $w.Quit() }}",
        ),
        "xls" | "xlsx" | "ods" | "csv" => format!(
            "$x = New-Object -ComObject Excel.Application; $x.Visible = $false; $x.DisplayAlerts = $false; \
             try {{ $wb = $x.Workbooks.Open('{inp}'); $wb.ExportAsFixedFormat(0, '{outp}'); $wb.Close($false) }} \
             finally {{ $x.Quit() }}",
        ),
        "ppt" | "pptx" | "odp" => format!(
            "$p = New-Object -ComObject PowerPoint.Application; \
             try {{ $pr = $p.Presentations.Open('{inp}', $true, $false, $false); $pr.SaveAs('{outp}', 32); $pr.Close() }} \
             finally {{ $p.Quit() }}",
        ),
        other => {
            return Err(format!(
                "MS Office não suporta \"{}\" para PDF aqui.",
                other
            ))
        }
    };

    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;

    if out.exists() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "MS Office não gerou o PDF. {}",
        stderr.trim()
    ))
}

pub(super) fn office_extract_text(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "log" => {
            std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| e.to_string())
        }
        "csv" => std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| e.to_string()),
        "html" | "htm" => {
            let raw = std::fs::read(path).map_err(|e| e.to_string())?;
            Ok(strip_markup(&String::from_utf8_lossy(&raw)))
        }
        "rtf" => {
            let raw = std::fs::read(path).map_err(|e| e.to_string())?;
            Ok(rtf_to_text(&String::from_utf8_lossy(&raw)))
        }
        "docx" => office_xml_to_text(&read_zip_entry(path, "word/document.xml")?),
        "odt" | "odp" => office_xml_to_text(&read_zip_entry(path, "content.xml")?),
        "pptx" => {
            let slides = read_zip_entries(path, "ppt/slides/slide", ".xml")?;
            office_xml_to_text(&slides)
        }
        "epub" => {
            let pages = read_zip_entries_by_suffix(path, &[".xhtml", ".html", ".htm"])?;
            Ok(strip_markup(&pages))
        }
        "xlsx" | "xls" | "ods" => spreadsheet_to_text(path),
        other => Err(format!(
            "Formato \"{}\" não é suportado na conversão nativa.",
            other
        )),
    }
}

pub(super) fn read_zip_entry(path: &Path, name: &str) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("entrada \"{}\" não encontrada no arquivo", name))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

pub(super) fn read_zip_entries(path: &Path, prefix: &str, suffix: &str) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| n.starts_with(prefix) && n.ends_with(suffix))
        .collect();
    names.sort();
    let mut out = String::new();
    for name in names {
        if let Ok(mut entry) = archive.by_name(&name) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                out.push_str(&buf);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

pub(super) fn read_zip_entries_by_suffix(path: &Path, suffixes: &[&str]) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| suffixes.iter().any(|s| n.to_lowercase().ends_with(s)))
        .collect();
    names.sort();
    let mut out = String::new();
    for name in names {
        if let Ok(mut entry) = archive.by_name(&name) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                out.push_str(&buf);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

pub(super) fn spreadsheet_to_text(path: &Path) -> Result<String, String> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names: Vec<String> = wb.sheet_names().to_vec();
    let mut out = String::new();
    for name in names {
        if let Ok(range) = wb.worksheet_range(&name) {
            if range.is_empty() {
                continue;
            }
            out.push_str(&format!("# {}\n", name));
            for row in range.rows() {
                let cells: Vec<String> = row
                    .iter()
                    .map(|c| match c {
                        Data::Empty => String::new(),
                        Data::String(s) => s.clone(),
                        Data::Float(f) => format!("{}", f),
                        Data::Int(i) => format!("{}", i),
                        Data::Bool(b) => format!("{}", b),
                        Data::DateTime(d) => format!("{}", d),
                        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
                        Data::Error(e) => format!("{:?}", e),
                    })
                    .collect();
                out.push_str(&cells.join(" | "));
                out.push('\n');
            }
            out.push('\n');
        }
    }
    Ok(out)
}

fn office_xml_to_text(xml: &str) -> Result<String, String> {
    let mut s = xml.to_string();
    for para in ["</w:p>", "</text:p>", "</text:h>", "</a:p>", "</p>"] {
        s = s.replace(para, "\n");
    }
    for tab in ["<w:tab/>", "<w:tab />", "<text:tab/>", "<text:tab/>"] {
        s = s.replace(tab, "\t");
    }
    for br in ["<w:br/>", "<w:br />", "<text:line-break/>", "<br/>", "<br />", "<br>"] {
        s = s.replace(br, "\n");
    }
    Ok(unescape_entities(&strip_tags(&s)))
}

fn strip_markup(html: &str) -> String {
    let mut s = html.to_string();
    for br in [
        "</p>", "</div>", "</li>", "</tr>", "</h1>", "</h2>", "</h3>", "</h4>", "<br/>", "<br />",
        "<br>",
    ] {
        s = s.replace(br, "\n");
    }
    let no_tags = strip_tags(&s);
    let text = unescape_entities(&no_tags);
    let mut blank = 0;
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

pub(super) fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub(super) fn unescape_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut ent = String::new();
        while let Some(&nc) = chars.peek() {
            if nc == ';' {
                chars.next();
                break;
            }
            if ent.len() > 10 {
                break;
            }
            ent.push(nc);
            chars.next();
        }
        match ent.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            "nbsp" => out.push(' '),
            _ => {
                if let Some(rest) = ent.strip_prefix('#') {
                    let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        rest.parse::<u32>().ok()
                    };
                    if let Some(ch) = code.and_then(char::from_u32) {
                        out.push(ch);
                    }
                } else {
                    out.push('&');
                    out.push_str(&ent);
                    out.push(';');
                }
            }
        }
    }
    out
}

fn rtf_to_text(rtf: &str) -> String {
    let mut out = String::new();
    let mut chars = rtf.chars().peekable();
    let mut depth = 0i32;
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => depth = (depth - 1).max(0),
            '\\' => {
                let mut word = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphabetic() {
                        word.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut num = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_digit() || (num.is_empty() && nc == '-') {
                        num.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Some(&' ') = chars.peek() {
                    chars.next();
                }
                match word.as_str() {
                    "par" | "line" => out.push('\n'),
                    "tab" => out.push('\t'),
                    _ => {}
                }
            }
            _ if depth >= 0 => out.push(c),
            _ => {}
        }
    }
    out
}
