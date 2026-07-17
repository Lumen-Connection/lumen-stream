pub fn sanitize_filename(name: &str) -> String {
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0'];
    let mut sanitized: String = name
        .chars()
        .filter(|c| !invalid_chars.contains(c) && !c.is_control())
        .collect();
    sanitized.truncate(200);
    if sanitized.trim().is_empty() {
        sanitized = "download".to_string();
    }
    sanitized
}

pub fn smart_clean_name(title: &str) -> String {
    const JUNK: &[&str] = &[
        "official", "oficial", "video", "vídeo", "audio", "áudio", "lyric", "letra",
        "lyrics", "hd", "4k", "8k", "mv", "m/v", "clipe", "visualizer", "remaster",
        "remastered", "explicit", "full album", "hq",
    ];
    let chars: Vec<char> = title.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let close = match chars[i] {
            '[' => Some(']'),
            '(' => Some(')'),
            '{' => Some('}'),
            _ => None,
        };
        if let Some(cl) = close {
            if let Some(j) = (i + 1..chars.len()).find(|&k| chars[k] == cl) {
                let inner: String = chars[i + 1..j].iter().collect::<String>().to_lowercase();
                if JUNK.iter().any(|k| inner.contains(k)) {
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    let out = out.replace(" - Topic", "").replace("- Topic", "");
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim()
        .trim_matches(|c| c == '-' || c == '|' || c == '·' || c == '_')
        .trim()
        .to_string();
    if trimmed.is_empty() {
        title.trim().to_string()
    } else {
        trimmed
    }
}

pub fn apply_template(template: &str, title: &str, channel: &str) -> String {
    let mut s = template.replace("%(title)s", title);
    s = s.replace("%(uploader)s", channel).replace("%(channel)s", channel);
    if s.trim().is_empty() {
        s = title.to_string();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_removes_invalid_chars() {
        assert_eq!(sanitize_filename("a<b>c:d\"e/f\\g|h?i*j"), "abcdefghij");
        assert_eq!(sanitize_filename("com\tcontrole\n"), "comcontrole");
    }

    #[test]
    fn sanitize_truncates_to_200() {
        let long = "x".repeat(250);
        assert_eq!(sanitize_filename(&long).len(), 200);
    }

    #[test]
    fn sanitize_empty_falls_back_to_download() {
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("???"), "download");
        assert_eq!(sanitize_filename("   "), "download");
    }

    #[test]
    fn smart_clean_removes_junk_brackets() {
        assert_eq!(
            smart_clean_name("Artist - Song (Official Video) [HD]"),
            "Artist - Song"
        );
        assert_eq!(smart_clean_name("Song [4K Remaster]"), "Song");
    }

    #[test]
    fn smart_clean_keeps_meaningful_brackets() {
        assert_eq!(
            smart_clean_name("Song (feat. Someone)"),
            "Song (feat. Someone)"
        );
    }

    #[test]
    fn smart_clean_removes_topic_suffix_and_collapses_spaces() {
        assert_eq!(smart_clean_name("Artist - Topic"), "Artist");
        assert_eq!(smart_clean_name("A   B    C"), "A B C");
    }

    #[test]
    fn smart_clean_falls_back_when_everything_is_junk() {
        // Se a limpeza esvazia o título, devolve o original (aparado).
        assert_eq!(smart_clean_name("(Official Video)"), "(Official Video)");
    }

    #[test]
    fn template_replaces_placeholders() {
        assert_eq!(
            apply_template("%(title)s - %(uploader)s", "T", "C"),
            "T - C"
        );
        assert_eq!(apply_template("%(channel)s", "T", "C"), "C");
    }

    #[test]
    fn empty_template_falls_back_to_title() {
        assert_eq!(apply_template("   ", "T", "C"), "T");
    }
}
