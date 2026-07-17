pub fn is_valid_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://") || u.starts_with("https://")
}

pub fn looks_like_url(url: &str) -> bool {
    let u = url.trim();
    !u.is_empty() && !u.contains(char::is_whitespace) && u.contains('.')
}

pub fn friendly_error(stderr: &str) -> String {
    let low = stderr.to_lowercase();
    let known = if low.contains("private video") || low.contains("sign in to confirm") {
        Some("Vídeo privado ou que exige login.")
    } else if low.contains("confirm your age") || low.contains("age-restricted") || low.contains("age restricted") {
        Some("Conteúdo com restrição de idade (requer login/cookies).")
    } else if low.contains("video unavailable") || low.contains("this video is not available") {
        Some("Vídeo indisponível.")
    } else if low.contains("requested format is not available") {
        Some("Formato/resolução indisponível para este vídeo.")
    } else if low.contains("unsupported url") || low.contains("is not a valid url") {
        Some("Link não suportado.")
    } else if low.contains("http error 403") || low.contains("403 forbidden") {
        Some("Acesso negado (403). Tente atualizar o yt-dlp em Configurações.")
    } else if low.contains("http error 404") {
        Some("Conteúdo não encontrado (404).")
    } else if low.contains("getaddrinfo")
        || low.contains("failed to resolve")
        || low.contains("unable to download webpage")
        || low.contains("temporary failure in name resolution")
        || low.contains("connection")
    {
        Some("Falha de conexão. Verifique sua internet.")
    } else {
        None
    };

    match known {
        Some(msg) => msg.to_string(),
        None => {
            let last = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("erro desconhecido");
            format!("Falha no download: {}", last.trim())
        }
    }
}

/// Remove o prefixo de índice de formato ("1: [download] ...") que o yt-dlp usa
/// quando baixa dois formatos em paralelo (ex.: live DVR com vídeo+áudio).
fn strip_format_index(line: &str) -> &str {
    let t = line.trim_start();
    if let Some((idx, rest)) = t.split_once(": ") {
        if !idx.is_empty() && idx.len() <= 3 && idx.chars().all(|c| c.is_ascii_digit()) {
            return rest.trim_start();
        }
    }
    t
}

pub(super) fn parse_ytdlp_percent(line: &str) -> Option<f64> {
    let l = strip_format_index(line);
    if !l.starts_with("[download]") {
        return None;
    }
    let token = l.split_whitespace().find(|t| t.ends_with('%'))?;
    token
        .trim_end_matches('%')
        .parse::<f64>()
        .ok()
        .map(|v| (v / 100.0).clamp(0.0, 1.0))
}

pub(super) fn parse_ytdlp_speed(line: &str) -> Option<f64> {
    let l = strip_format_index(line);
    if !l.starts_with("[download]") {
        return None;
    }
    let tok = l.split_whitespace().find(|t| t.ends_with("/s"))?;
    let body = tok.trim_end_matches("/s");
    let split = body.find(|c: char| c.is_alphabetic()).unwrap_or(body.len());
    let (num, unit) = body.split_at(split);
    let value: f64 = num.parse().ok()?;
    let mult = match unit {
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "KiB" => 1024.0,
        "GB" => 1_000_000_000.0,
        "MB" => 1_000_000.0,
        "KB" | "kB" => 1000.0,
        "B" | "" => 1.0,
        _ => return None,
    };
    Some(value * mult)
}

pub(super) fn parse_ytdlp_eta(line: &str) -> Option<u64> {
    let l = strip_format_index(line);
    if !l.starts_with("[download]") {
        return None;
    }
    let mut it = l.split_whitespace();
    let tok = loop {
        match it.next() {
            Some("ETA") => break it.next()?,
            Some(_) => continue,
            None => return None,
        }
    };
    let mut secs = 0u64;
    for p in tok.split(':') {
        secs = secs * 60 + p.parse::<u64>().ok()?;
    }
    Some(secs)
}

/// Extrai o total baixado de uma linha do yt-dlp (ex.: "[download]  30.56MiB at ...").
pub(super) fn parse_ytdlp_size(line: &str) -> Option<u64> {
    let l = strip_format_index(line);
    if !l.starts_with("[download]") {
        return None;
    }
    for tok in l.split_whitespace() {
        let unit_mul = if let Some(n) = tok.strip_suffix("GiB") {
            Some((n, 1024.0 * 1024.0 * 1024.0))
        } else if let Some(n) = tok.strip_suffix("MiB") {
            Some((n, 1024.0 * 1024.0))
        } else if let Some(n) = tok.strip_suffix("KiB") {
            Some((n, 1024.0))
        } else {
            None
        };
        if let Some((num, mul)) = unit_mul {
            if let Ok(v) = num.parse::<f64>() {
                return Some((v * mul) as u64);
            }
        }
    }
    None
}

pub(super) fn ytdlp_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let last = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("erro desconhecido");
    format!("yt-dlp: {}", last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_url_requires_http_scheme() {
        assert!(is_valid_url("https://youtube.com/watch?v=x"));
        assert!(is_valid_url("  http://a.com  "));
        assert!(!is_valid_url("youtube.com/watch"));
        assert!(!is_valid_url(""));
    }

    #[test]
    fn looks_like_url_heuristic() {
        assert!(looks_like_url("youtube.com/watch"));
        assert!(!looks_like_url("apenas texto com espaço.com"));
        assert!(!looks_like_url("semponto"));
        assert!(!looks_like_url(""));
    }

    #[test]
    fn friendly_error_maps_known_patterns() {
        assert_eq!(
            friendly_error("ERROR: Private video"),
            "Vídeo privado ou que exige login."
        );
        assert_eq!(
            friendly_error("ERROR: Sign in to confirm your age"),
            "Vídeo privado ou que exige login."
        );
        assert_eq!(friendly_error("Video unavailable"), "Vídeo indisponível.");
        assert_eq!(
            friendly_error("HTTP Error 404: Not Found"),
            "Conteúdo não encontrado (404)."
        );
        assert_eq!(
            friendly_error("getaddrinfo failed"),
            "Falha de conexão. Verifique sua internet."
        );
    }

    #[test]
    fn friendly_error_falls_back_to_last_line() {
        assert_eq!(
            friendly_error("linha 1\nERROR: boom\n\n"),
            "Falha no download: ERROR: boom"
        );
        assert_eq!(friendly_error(""), "Falha no download: erro desconhecido");
    }

    #[test]
    fn percent_parses_download_lines() {
        let line = "[download]  45.2% of 100.00MiB at 1.00MiB/s ETA 00:30";
        assert_eq!(parse_ytdlp_percent(line), Some(0.452));
        // Prefixo de índice de formato (download paralelo de vídeo+áudio).
        assert_eq!(parse_ytdlp_percent(&format!("1: {line}")), Some(0.452));
    }

    #[test]
    fn percent_ignores_other_lines_and_clamps() {
        assert_eq!(parse_ytdlp_percent("[youtube] extraindo"), None);
        assert_eq!(parse_ytdlp_percent("[download] sem porcentagem"), None);
        assert_eq!(
            parse_ytdlp_percent("[download] 150.0% of ~10MiB"),
            Some(1.0)
        );
    }

    #[test]
    fn speed_parses_binary_and_decimal_units() {
        let l = |s: &str| format!("[download] 10.0% of 1MiB at {s} ETA 00:01");
        assert_eq!(parse_ytdlp_speed(&l("1.00MiB/s")), Some(1024.0 * 1024.0));
        assert_eq!(parse_ytdlp_speed(&l("500.00KiB/s")), Some(500.0 * 1024.0));
        assert_eq!(parse_ytdlp_speed(&l("2MB/s")), Some(2_000_000.0));
        assert_eq!(parse_ytdlp_speed("[youtube] x"), None);
    }

    #[test]
    fn eta_parses_mm_ss_and_hh_mm_ss() {
        assert_eq!(
            parse_ytdlp_eta("[download] 10.0% of 1MiB at 1MiB/s ETA 00:30"),
            Some(30)
        );
        assert_eq!(
            parse_ytdlp_eta("[download] 10.0% of 1MiB at 1MiB/s ETA 1:02:03"),
            Some(3723)
        );
        assert_eq!(parse_ytdlp_eta("[download] sem eta"), None);
    }

    #[test]
    fn size_parses_downloaded_total() {
        assert_eq!(
            parse_ytdlp_size("[download]  30.56MiB at 2.00MiB/s"),
            Some((30.56f64 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(parse_ytdlp_size("[download] 100% concluído"), None);
    }

    #[test]
    fn strip_format_index_only_removes_short_numeric_prefix() {
        assert_eq!(strip_format_index("1: [download] x"), "[download] x");
        assert_eq!(strip_format_index("[download] x"), "[download] x");
        // "1234: " não é índice de formato (mais de 3 dígitos).
        assert_eq!(strip_format_index("1234: resto"), "1234: resto");
    }

    #[test]
    fn ytdlp_error_uses_last_nonempty_line() {
        assert_eq!(ytdlp_error(b"aviso\nerro fatal\n\n"), "yt-dlp: erro fatal");
        assert_eq!(ytdlp_error(b""), "yt-dlp: erro desconhecido");
    }
}
