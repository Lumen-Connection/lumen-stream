//! Conversão Markdown (entrada e saída).
//!
//! 4A: documentos/office/pdf → `.md` preservando estrutura quando possível.
//! 4B: `.md` → pdf / html / txt.
//!
//! PDF de saída usa `render_text_pdf` (fonte/tamanho fixos) — títulos não
//! ganham hierarquia visual no PDF. Ver decisão pendente na Task 04.

use std::path::{Path, PathBuf};

use super::office::{
    read_zip_entries_by_suffix, read_zip_entry, spreadsheet_to_text, strip_tags, unescape_entities,
};
use super::pdf::render_text_pdf;
use super::DownloadEngine;

impl DownloadEngine {
    pub(super) async fn convert_to_markdown(
        &self,
        input: &Path,
        out: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let input = input.to_path_buf();
        let out_path = out.to_path_buf();
        let out_ret = out_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let md = document_to_markdown(&input)?;
            std::fs::write(&out_path, md).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok(out_ret)
    }

    pub(super) async fn convert_from_markdown(
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
            let src = std::fs::read_to_string(&input).map_err(|e| e.to_string())?;
            match format.as_str() {
                "html" => {
                    let html = markdown_to_html(&src, &title);
                    std::fs::write(&out_path, html).map_err(|e| e.to_string())
                }
                "txt" => {
                    let txt = markdown_to_plain(&src);
                    std::fs::write(&out_path, txt).map_err(|e| e.to_string())
                }
                "pdf" => {
                    // Sem hierarquia visual: render_text_pdf é fonte fixa.
                    let plain = markdown_to_plain(&src);
                    render_text_pdf(&plain, &out_path, &title)
                }
                other => Err(format!(
                    "Markdown só converte para pdf, html ou txt (recebido: {})",
                    other
                )),
            }
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok(out_ret)
    }
}

/// Converte um documento de office/pdf/html em Markdown estruturado.
pub(super) fn document_to_markdown(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "docx" => docx_to_markdown(path),
        "odt" => {
            let xml = read_zip_entry(path, "content.xml")?;
            Ok(html_like_to_markdown(&odt_xml_to_htmlish(&xml)))
        }
        "html" | "htm" => {
            let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(html_like_to_markdown(&raw))
        }
        "epub" => {
            let pages = read_zip_entries_by_suffix(path, &[".xhtml", ".html", ".htm"])?;
            Ok(html_like_to_markdown(&pages))
        }
        "xlsx" | "xls" | "ods" | "csv" => spreadsheet_to_gfm(path),
        "pptx" => pptx_to_markdown(path),
        "pdf" => pdf_to_markdown(path),
        "txt" | "rtf" | "doc" | "ppt" | "odp" => {
            // Fallback: texto plano escapado (sem estrutura rica disponível).
            let text = super::office::office_extract_text(path)?;
            Ok(escape_md_text(&text))
        }
        other => Err(format!(
            "Conversão para Markdown não suportada a partir de .{}",
            other
        )),
    }
}

/// Escapa `* _ # [ ]` no texto corrido.
pub(super) fn escape_md_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '*' | '_' | '#' | '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn spreadsheet_to_gfm(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "csv" {
        let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        return Ok(csv_to_gfm(&raw));
    }
    // spreadsheet_to_text emite `# Planilha` + linhas `a | b | c`.
    // Inserimos a linha separadora GFM após a primeira linha de dados de cada sheet.
    let text = spreadsheet_to_text(path)?;
    Ok(inject_gfm_separators(&text))
}

/// Transforma blocos `a | b | c` em tabela GFM válida (com `---`).
pub(super) fn inject_gfm_separators(text: &str) -> String {
    let mut out = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        if line.starts_with('#') {
            continue;
        }
        if line.contains(" | ") || (line.contains('|') && !line.trim().is_empty()) {
            // Primeira linha de dados do bloco: injeta separador se a próxima
            // também parece ser linha de tabela (ou fim do bloco).
            let cols = line.split('|').count().max(1);
            let sep: String = (0..cols).map(|_| "---").collect::<Vec<_>>().join(" | ");
            out.push_str(&sep);
            out.push('\n');
            // Copia o resto do bloco sem re-inserir.
            while let Some(next) = lines.peek() {
                if next.trim().is_empty() || next.starts_with('#') {
                    break;
                }
                out.push_str(lines.next().unwrap());
                out.push('\n');
            }
        }
    }
    out
}

pub(super) fn csv_to_gfm(csv: &str) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in csv.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<String> = line
            .split(',')
            .map(|c| c.trim().trim_matches('"').to_string())
            .collect();
        rows.push(cells);
    }
    if rows.is_empty() {
        return String::new();
    }
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(1);
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        let mut cells = row.clone();
        while cells.len() < width {
            cells.push(String::new());
        }
        out.push_str(&cells.join(" | "));
        out.push('\n');
        if i == 0 {
            let sep: String = (0..width).map(|_| "---").collect::<Vec<_>>().join(" | ");
            out.push_str(&sep);
            out.push('\n');
        }
    }
    out
}

fn pptx_to_markdown(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        .collect();
    names.sort();
    let mut out = String::new();
    for (i, name) in names.iter().enumerate() {
        out.push_str(&format!("## Slide {}\n\n", i + 1));
        if let Ok(mut entry) = archive.by_name(name) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                // Extrai <a:t>…</a:t> como parágrafos simples.
                for piece in extract_xml_text_nodes(&buf, "a:t") {
                    let t = piece.trim();
                    if !t.is_empty() {
                        out.push_str(&escape_md_text(t));
                        out.push('\n');
                    }
                }
            }
        }
        out.push('\n');
    }
    Ok(out)
}

fn pdf_to_markdown(path: &Path) -> Result<String, String> {
    let doc = printpdf::lopdf::Document::load(path).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();
    let page_numbers: Vec<u32> = pages.keys().copied().collect();
    let text = doc
        .extract_text(&page_numbers)
        .map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Err(
            "Nenhum texto encontrado (o PDF pode ser apenas imagens escaneadas).".into(),
        );
    }
    Ok(heuristic_paragraphs(&text))
}

/// Junta linhas quebradas em parágrafos e preserva blocos em branco.
fn heuristic_paragraphs(text: &str) -> String {
    let mut out = String::new();
    let mut para = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            if !para.is_empty() {
                out.push_str(&escape_md_text(para.trim()));
                out.push_str("\n\n");
                para.clear();
            }
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(t);
        }
    }
    if !para.is_empty() {
        out.push_str(&escape_md_text(para.trim()));
        out.push('\n');
    }
    out
}

/// DOCX → Markdown com headings, bold/italic, listas e tabelas GFM.
pub(super) fn docx_to_markdown(path: &Path) -> Result<String, String> {
    let xml = read_zip_entry(path, "word/document.xml")?;
    Ok(docx_xml_to_markdown(&xml))
}

pub(super) fn docx_xml_to_markdown(xml: &str) -> String {
    let mut out = String::new();
    // Itera por parágrafos <w:p>…</w:p> e tabelas <w:tbl>…</w:tbl>.
    let mut rest = xml;
    while let Some(pos) = find_next_block(rest) {
        match pos {
            Block::Para(start) => {
                let after = &rest[start..];
                if let Some(end) = after.find("</w:p>") {
                    let body = &after[..end + 6];
                    out.push_str(&docx_paragraph_md(body));
                    rest = &after[end + 6..];
                } else {
                    break;
                }
            }
            Block::Table(start) => {
                let after = &rest[start..];
                if let Some(end) = after.find("</w:tbl>") {
                    let body = &after[..end + 8];
                    out.push_str(&docx_table_md(body));
                    out.push('\n');
                    rest = &after[end + 8..];
                } else {
                    break;
                }
            }
        }
    }
    out
}

enum Block {
    Para(usize),
    Table(usize),
}

fn find_next_block(s: &str) -> Option<Block> {
    let p = s.find("<w:p");
    let t = s.find("<w:tbl");
    match (p, t) {
        (Some(pi), Some(ti)) if ti < pi => Some(Block::Table(ti)),
        (Some(pi), _) => Some(Block::Para(pi)),
        (None, Some(ti)) => Some(Block::Table(ti)),
        _ => None,
    }
}

fn docx_paragraph_md(p_xml: &str) -> String {
    let heading = docx_heading_level(p_xml);
    let is_list = p_xml.contains("<w:numPr");
    // Numérico se ilvl+numId presentes e abstractNum sugere decimal — heurística:
    // se contém w:numFmt val="decimal" no mesmo trecho (raro no document.xml),
    // senão bullet.
    let ordered = p_xml.contains("val=\"decimal\"");

    let mut text = String::new();
    let mut rest = p_xml;
    while let Some(rstart) = rest.find("<w:r") {
        let after = &rest[rstart..];
        let Some(rend) = after.find("</w:r>") else { break };
        let run = &after[..rend + 6];
        let bold = run.contains("<w:b") && !run.contains("<w:b w:val=\"0\"") && !run.contains("<w:b w:val=\"false\"");
        let italic = run.contains("<w:i")
            && !run.contains("<w:i w:val=\"0\"")
            && !run.contains("<w:i w:val=\"false\"");
        let mut piece = String::new();
        for t in extract_xml_text_nodes(run, "w:t") {
            piece.push_str(&t);
        }
        if piece.is_empty() {
            rest = &after[rend + 6..];
            continue;
        }
        let escaped = escape_md_text(&piece);
        match (bold, italic) {
            (true, true) => {
                text.push_str("***");
                text.push_str(&escaped);
                text.push_str("***");
            }
            (true, false) => {
                text.push_str("**");
                text.push_str(&escaped);
                text.push_str("**");
            }
            (false, true) => {
                text.push('*');
                text.push_str(&escaped);
                text.push('*');
            }
            (false, false) => text.push_str(&escaped),
        }
        rest = &after[rend + 6..];
    }

    let t = text.trim();
    if t.is_empty() {
        return "\n".into();
    }
    if let Some(level) = heading {
        let hashes = "#".repeat(level.clamp(1, 6));
        return format!("{} {}\n\n", hashes, t);
    }
    if is_list {
        if ordered {
            return format!("1. {}\n", t);
        }
        return format!("- {}\n", t);
    }
    format!("{}\n\n", t)
}

fn docx_heading_level(p_xml: &str) -> Option<usize> {
    // w:pStyle w:val="Heading1" … "Heading6" (também "Título1" em PT-BR Word).
    let markers = [
        ("Heading1", 1),
        ("Heading2", 2),
        ("Heading3", 3),
        ("Heading4", 4),
        ("Heading5", 5),
        ("Heading6", 6),
        ("Título1", 1),
        ("Título2", 2),
        ("Título3", 3),
        ("Title", 1),
        ("Subtitle", 2),
    ];
    for (name, level) in markers {
        if p_xml.contains(&format!("w:val=\"{}\"", name)) {
            return Some(level);
        }
    }
    None
}

fn docx_table_md(tbl_xml: &str) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut rest = tbl_xml;
    while let Some(start) = rest.find("<w:tr") {
        let after = &rest[start..];
        let Some(end) = after.find("</w:tr>") else { break };
        let row_xml = &after[..end + 7];
        let mut cells = Vec::new();
        let mut r2 = row_xml;
        while let Some(cs) = r2.find("<w:tc") {
            let a2 = &r2[cs..];
            let Some(ce) = a2.find("</w:tc>") else { break };
            let cell = &a2[..ce + 7];
            let mut cell_text = String::new();
            for t in extract_xml_text_nodes(cell, "w:t") {
                if !cell_text.is_empty() {
                    cell_text.push(' ');
                }
                cell_text.push_str(t.trim());
            }
            cells.push(escape_md_text(cell_text.trim()));
            r2 = &a2[ce + 7..];
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
        rest = &after[end + 7..];
    }
    if rows.is_empty() {
        return String::new();
    }
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(1);
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        let mut cells = row.clone();
        while cells.len() < width {
            cells.push(String::new());
        }
        out.push_str(&cells.join(" | "));
        out.push('\n');
        if i == 0 {
            let sep: String = (0..width).map(|_| "---").collect::<Vec<_>>().join(" | ");
            out.push_str(&sep);
            out.push('\n');
        }
    }
    out
}

fn extract_xml_text_nodes<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    // Evita que `<w:t` case em `<w:tc` — exige fim de nome de tag.
    let open_bare = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = rest.find(&open_bare) {
        let after = &rest[s..];
        // Caractere logo após o nome da tag deve ser espaço, `>` ou `/`.
        let after_name = &after[open_bare.len()..];
        let ok = after_name
            .chars()
            .next()
            .map(|c| c == '>' || c == '/' || c.is_whitespace())
            .unwrap_or(false);
        if !ok {
            rest = &after[open_bare.len()..];
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        // self-closing?
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let content_start = gt + 1;
        let Some(end) = after[content_start..].find(&close) else { break };
        out.push(&after[content_start..content_start + end]);
        rest = &after[content_start + end + close.len()..];
    }
    out
}

fn odt_xml_to_htmlish(xml: &str) -> String {
    // Converte tags ODT comuns para HTML genérico que html_like_to_markdown entende.
    let mut s = xml.to_string();
    for (a, b) in [
        ("text:h text:outline-level=\"1\"", "h1"),
        ("text:h text:outline-level=\"2\"", "h2"),
        ("text:h text:outline-level=\"3\"", "h3"),
        ("</text:h>", "</h1>"),
        ("</text:p>", "</p>"),
        ("<text:p", "<p"),
        ("</text:list-item>", "</li>"),
        ("<text:list-item", "<li"),
        ("</text:list>", "</ul>"),
        ("<text:list", "<ul"),
        ("text:span text:style-name=\"Strong\"", "strong"),
        ("</text:span>", "</strong>"),
    ] {
        s = s.replace(a, b);
    }
    s
}

/// HTML (ou subset) → Markdown.
pub(super) fn html_like_to_markdown(html: &str) -> String {
    let mut s = html.to_string();
    // Remove scripts/styles.
    s = strip_tag_block(&s, "script");
    s = strip_tag_block(&s, "style");

    for level in 1..=6 {
        let open = format!("<h{}", level);
        let close = format!("</h{}>", level);
        s = replace_heading(&s, &open, &close, level);
    }
    s = s.replace("<strong>", "**").replace("</strong>", "**");
    s = s.replace("<b>", "**").replace("</b>", "**");
    s = s.replace("<em>", "*").replace("</em>", "*");
    s = s.replace("<i>", "*").replace("</i>", "*");
    s = s.replace("<li>", "- ").replace("</li>", "\n");
    s = s.replace("<ul>", "\n").replace("</ul>", "\n");
    s = s.replace("<ol>", "\n").replace("</ol>", "\n");
    s = s.replace("<br/>", "\n").replace("<br />", "\n").replace("<br>", "\n");
    s = s.replace("<p>", "\n").replace("</p>", "\n\n");
    s = s.replace("<tr>", "").replace("</tr>", "\n");
    s = s.replace("<td>", "| ").replace("</td>", " ");
    s = s.replace("<th>", "| ").replace("</th>", " ");
    s = s.replace("<table>", "\n").replace("</table>", "\n");

    // Links: <a href="url">text</a> → [text](url)
    s = convert_anchors(&s);

    let text = unescape_entities(&strip_tags(&s));
    // Normaliza linhas em branco.
    let mut blank = 0;
    let mut out = String::new();
    for line in text.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            blank += 1;
            if blank <= 2 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(t.trim_start());
            out.push('\n');
        }
    }
    out
}

fn strip_tag_block(s: &str, tag: &str) -> String {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        if let Some(end) = after.to_lowercase().find(&close) {
            rest = &after[end + close.len()..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

fn replace_heading(s: &str, open_prefix: &str, close: &str, level: usize) -> String {
    let hashes = "#".repeat(level);
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(open_prefix) {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let content_start = gt + 1;
        if let Some(end) = after[content_start..].find(close) {
            let inner = &after[content_start..content_start + end];
            let plain = strip_tags(inner).trim().to_string();
            out.push_str(&format!("\n{} {}\n\n", hashes, plain));
            rest = &after[content_start + end + close.len()..];
        } else {
            out.push_str(after);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn convert_anchors(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("<a ") {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let tag = &after[..gt + 1];
        let href = attr_value(tag, "href").unwrap_or_default();
        let content_start = gt + 1;
        if let Some(end) = after[content_start..].find("</a>") {
            let inner = strip_tags(&after[content_start..content_start + end]);
            out.push_str(&format!("[{}]({})", inner.trim(), href));
            rest = &after[content_start + end + 4..];
        } else {
            out.push_str(after);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let key = format!("{}=\"", name);
    let i = tag.find(&key)?;
    let rest = &tag[i + key.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ─── Markdown → HTML / plain ───────────────────────────────────────────────

pub(super) fn markdown_to_html(md: &str, title: &str) -> String {
    let mut body = String::new();
    let mut in_code = false;
    let mut in_ul = false;
    let mut in_ol = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    let flush_lists = |body: &mut String, in_ul: &mut bool, in_ol: &mut bool| {
        if *in_ul {
            body.push_str("</ul>\n");
            *in_ul = false;
        }
        if *in_ol {
            body.push_str("</ol>\n");
            *in_ol = false;
        }
    };
    let flush_table = |body: &mut String, rows: &mut Vec<Vec<String>>| {
        if rows.is_empty() {
            return;
        }
        body.push_str("<table>\n");
        for (i, row) in rows.iter().enumerate() {
            body.push_str("<tr>");
            let tag = if i == 0 { "th" } else { "td" };
            for c in row {
                body.push_str(&format!("<{tag}>{}</{tag}>", html_escape(c)));
            }
            body.push_str("</tr>\n");
        }
        body.push_str("</table>\n");
        rows.clear();
    };

    for line in md.lines() {
        let t = line.trim_end();
        if t.starts_with("```") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            flush_table(&mut body, &mut table_rows);
            if in_code {
                body.push_str("</code></pre>\n");
                in_code = false;
            } else {
                body.push_str("<pre><code>");
                in_code = true;
            }
            continue;
        }
        if in_code {
            body.push_str(&html_escape(t));
            body.push('\n');
            continue;
        }

        // Tabela GFM
        if t.contains('|') && !t.trim().is_empty() {
            let cells: Vec<String> = t
                .split('|')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty() || true)
                .collect();
            // Linha separadora |---|---|
            if cells.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '))
                && cells.iter().any(|c| c.contains('-'))
            {
                continue;
            }
            let cells: Vec<String> = t
                .trim()
                .trim_matches('|')
                .split('|')
                .map(|c| inline_md_to_html(c.trim()))
                .collect();
            table_rows.push(cells);
            continue;
        } else {
            flush_table(&mut body, &mut table_rows);
        }

        if let Some(rest) = t.strip_prefix("###### ") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<h6>{}</h6>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = t.strip_prefix("##### ") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<h5>{}</h5>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = t.strip_prefix("#### ") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<h4>{}</h4>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = t.strip_prefix("### ") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<h3>{}</h3>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = t.strip_prefix("## ") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<h2>{}</h2>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = t.strip_prefix("# ") {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<h1>{}</h1>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            if !in_ul {
                flush_lists(&mut body, &mut in_ul, &mut in_ol);
                body.push_str("<ul>\n");
                in_ul = true;
            }
            body.push_str(&format!("<li>{}</li>\n", inline_md_to_html(rest)));
        } else if let Some(rest) = ordered_list_item(t) {
            if !in_ol {
                flush_lists(&mut body, &mut in_ul, &mut in_ol);
                body.push_str("<ol>\n");
                in_ol = true;
            }
            body.push_str(&format!("<li>{}</li>\n", inline_md_to_html(rest)));
        } else if t.trim().is_empty() {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
        } else {
            flush_lists(&mut body, &mut in_ul, &mut in_ol);
            body.push_str(&format!("<p>{}</p>\n", inline_md_to_html(t)));
        }
    }
    flush_lists(&mut body, &mut in_ul, &mut in_ol);
    flush_table(&mut body, &mut table_rows);

    format!(
        r#"<!DOCTYPE html>
<html lang="pt-BR">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{title}</title>
<style>
body {{ font-family: system-ui, Segoe UI, sans-serif; max-width: 720px; margin: 2rem auto; padding: 0 1rem; line-height: 1.55; color: #1a1a1a; }}
code, pre {{ background: #f4f4f5; border-radius: 6px; }}
pre {{ padding: 1rem; overflow: auto; }}
code {{ padding: 0.1em 0.35em; }}
table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; }}
th, td {{ border: 1px solid #ccc; padding: 0.4rem 0.6rem; text-align: left; }}
th {{ background: #f0f0f0; }}
a {{ color: #2563eb; }}
</style>
</head>
<body>
{body}
</body>
</html>
"#,
        title = html_escape(title),
        body = body
    )
}

fn ordered_list_item(t: &str) -> Option<&str> {
    let mut chars = t.chars();
    let mut saw_digit = false;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        if c == '.' && saw_digit {
            let rest = chars.as_str();
            return rest.strip_prefix(' ').or(Some(rest));
        }
        return None;
    }
    None
}

fn inline_md_to_html(s: &str) -> String {
    let mut out = html_escape(s);
    // **bold**
    out = replace_delimited(&out, "**", "<strong>", "</strong>");
    // *italic*
    out = replace_delimited(&out, "*", "<em>", "</em>");
    // `code`
    out = replace_delimited(&out, "`", "<code>", "</code>");
    // [text](url)
    out = convert_md_links(&out);
    out
}

fn replace_delimited(s: &str, delim: &str, open: &str, close: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(delim) {
        out.push_str(&rest[..i]);
        let after = &rest[i + delim.len()..];
        if let Some(j) = after.find(delim) {
            out.push_str(open);
            out.push_str(&after[..j]);
            out.push_str(close);
            rest = &after[j + delim.len()..];
        } else {
            out.push_str(delim);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn convert_md_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(mid) = after.find("](") else {
            out.push('[');
            rest = after;
            continue;
        };
        let text = &after[..mid];
        let after_url = &after[mid + 2..];
        let Some(end) = after_url.find(')') else {
            out.push('[');
            rest = after;
            continue;
        };
        let url = &after_url[..end];
        out.push_str(&format!("<a href=\"{}\">{}</a>", url, text));
        rest = &after_url[end + 1..];
    }
    out.push_str(rest);
    out
}

pub(super) fn markdown_to_plain(md: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in md.lines() {
        let t = line.trim_end();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push_str(t);
            out.push('\n');
            continue;
        }
        let mut s = t.to_string();
        for prefix in ["###### ", "##### ", "#### ", "### ", "## ", "# ", "- ", "* "] {
            if let Some(r) = s.strip_prefix(prefix) {
                s = r.to_string();
                break;
            }
        }
        if let Some(r) = ordered_list_item(&s) {
            s = r.to_string();
        }
        // Remove ênfase e links simples.
        s = s.replace("**", "").replace("__", "");
        s = strip_md_links(&s);
        s = s.replace('`', "");
        // Separador de tabela.
        if s.chars().all(|c| c == '-' || c == '|' || c == ':' || c == ' ') && s.contains('-') {
            continue;
        }
        s = s.replace('|', " ");
        out.push_str(s.trim());
        out.push('\n');
    }
    out
}

fn strip_md_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(mid) = after.find("](") else {
            out.push('[');
            rest = after;
            continue;
        };
        let text = &after[..mid];
        let after_url = &after[mid + 2..];
        if let Some(end) = after_url.find(')') {
            out.push_str(text);
            rest = &after_url[end + 1..];
        } else {
            out.push('[');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_special_chars() {
        assert_eq!(escape_md_text("a*b_c#d[e]"), r"a\*b\_c\#d\[e\]");
    }

    #[test]
    fn docx_heading_bold_list_table() {
        let xml = r#"
        <w:document><w:body>
          <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Titulo</w:t></w:r></w:p>
          <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>negrito</w:t></w:r></w:p>
          <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>item</w:t></w:r></w:p>
          <w:tbl>
            <w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr>
            <w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr>
          </w:tbl>
        </w:body></w:document>
        "#;
        let md = docx_xml_to_markdown(xml);
        assert!(md.contains("# Titulo"), "{md}");
        assert!(md.contains("**negrito**"), "{md}");
        assert!(md.contains("- item"), "{md}");
        assert!(md.contains("A | B"), "{md}");
        assert!(md.contains("---"), "{md}");
    }

    #[test]
    fn spreadsheet_gfm_has_separator() {
        let text = "# Sheet1\na | b | c\n1 | 2 | 3\n\n";
        let gfm = inject_gfm_separators(text);
        assert!(gfm.contains("---"), "{gfm}");
        assert!(gfm.contains("a | b | c"), "{gfm}");
    }

    #[test]
    fn csv_to_gfm_valid() {
        let gfm = csv_to_gfm("a,b\n1,2\n");
        assert!(gfm.lines().nth(1).unwrap().contains("---"));
    }

    #[test]
    fn html_headings_and_links() {
        let md = html_like_to_markdown("<h1>Hi</h1><p>go <a href=\"https://x.com\">here</a></p>");
        assert!(md.contains("# Hi"), "{md}");
        assert!(md.contains("[here](https://x.com)"), "{md}");
    }

    #[test]
    fn md_to_html_heading_list_table() {
        let html = markdown_to_html(
            "# Title\n\n- one\n- two\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n",
            "Doc",
        );
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<ul>"), "{html}");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("<th>A</th>") || html.contains("<th>A"), "{html}");
    }

    #[test]
    fn md_to_plain_strips_markers() {
        let t = markdown_to_plain("# Hi\n\n**bold** and [x](http://y)\n");
        assert!(t.contains("Hi"), "{t}");
        assert!(t.contains("bold"), "{t}");
        assert!(!t.contains("**"), "{t}");
        assert!(t.contains("x"), "{t}");
        assert!(!t.contains("http"), "{t}");
    }
}
