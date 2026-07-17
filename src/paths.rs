use std::path::PathBuf;

/// Diretório de dados do app em AppData (config, banco, libs, thumbs, log).
///
/// Migra automaticamente a pasta antiga "LumenDownloader" para "LumenStream"
/// na primeira chamada (rebrand). É idempotente e best-effort: se o rename
/// falhar, cai de volta no nome antigo — nunca perde dados nem começa do zero.
pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let new = base.join("LumenStream");
    let old = base.join("LumenDownloader");
    if new.exists() {
        new
    } else if old.exists() {
        match std::fs::rename(&old, &new) {
            Ok(_) => new,
            Err(_) => old,
        }
    } else {
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // data_dir depende da máquina; o invariante testável é o contrato do
    // rebrand: sempre uma das duas pastas, e estável entre chamadas.
    #[test]
    fn data_dir_is_stable_and_uses_known_names() {
        let d = data_dir();
        let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(
            name == "LumenStream" || name == "LumenDownloader",
            "nome inesperado: {name}"
        );
        assert_eq!(data_dir(), d, "chamadas repetidas devem ser idempotentes");
    }
}
