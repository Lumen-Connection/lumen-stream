
use crate::app::{App, Tab};
use crate::config::settings::Theme;
use crate::ui::theme;

#[derive(Clone, Copy)]
pub enum Cmd {
    Go(Tab),
    UpdateYtdlp,
    OpenDownloads,
    ToggleTheme,
    ClearTemp,
}

pub fn all_commands(pt: bool) -> Vec<(String, Cmd)> {
    let go = |icon: &str, name: &str, tab: Tab| (format!("{}  {}", icon, name), Cmd::Go(tab));
    let mut cmds = vec![
        go("🏠", if pt { "Início" } else { "Home" }, Tab::Home),
        go("🎵", if pt { "Baixar Música" } else { "Download Music" }, Tab::Music),
        go("🎬", if pt { "Baixar Vídeo" } else { "Download Video" }, Tab::Video),
        go("🔄", "Converter", Tab::Converter),
        go("📋", if pt { "Fila" } else { "Queue" }, Tab::Queue),
        go("📁", if pt { "Pastas" } else { "Folders" }, Tab::Folders),
        go("🎮", if pt { "Sincronizar Jogos" } else { "Sync to Games" }, Tab::Games),
        go("☁", if pt { "Nuvem" } else { "Cloud" }, Tab::Cloud),
        go("🏆", if pt { "Conquistas" } else { "Achievements" }, Tab::Achievements),
        go("⚙", if pt { "Configurações" } else { "Settings" }, Tab::Settings),
        go("❓", if pt { "Ajuda" } else { "Help" }, Tab::Help),
        (
            format!("⬆  {}", if pt { "Atualizar yt-dlp" } else { "Update yt-dlp" }),
            Cmd::UpdateYtdlp,
        ),
        (
            format!("📂  {}", if pt { "Abrir pasta de downloads" } else { "Open downloads folder" }),
            Cmd::OpenDownloads,
        ),
        (
            format!("🌓  {}", if pt { "Alternar tema claro/escuro" } else { "Toggle light/dark theme" }),
            Cmd::ToggleTheme,
        ),
        (
            format!("🧹  {}", if pt { "Limpar arquivos temporários" } else { "Clear temp files" }),
            Cmd::ClearTemp,
        ),
    ];
    cmds.retain(|(_, cmd)| match cmd {
        Cmd::Go(tab) => tab.visible(),
        _ => true,
    });
    cmds
}

pub fn run(app: &mut App, cmd: Cmd) {
    match cmd {
        Cmd::Go(tab) => app.active_tab = tab,
        Cmd::UpdateYtdlp => {
            app.active_tab = Tab::Settings;
            crate::ui::settings_tab::start_update(app);
        }
        Cmd::OpenDownloads => {
            std::fs::create_dir_all(&app.config.default_download_dir).ok();
            open::that(&app.config.default_download_dir).ok();
        }
        Cmd::ToggleTheme => {
            let new = if app.config.theme == Theme::Light {
                Theme::Dark
            } else {
                Theme::Light
            };
            app.config.theme = new;
            theme::set_light(new == Theme::Light);
            app.config.save();
            app.restyle = true;
        }
        Cmd::ClearTemp => app.clear_temp_files_toast(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_same_commands_in_both_languages() {
        let pt = all_commands(true);
        let en = all_commands(false);
        assert_eq!(pt.len(), en.len());
        assert!(!pt.is_empty());
        assert!(
            pt.iter().zip(&en).any(|((a, _), (b, _))| a != b),
            "os rótulos traduzidos devem diferir entre os idiomas"
        );
    }

    #[test]
    fn palette_only_lists_visible_tabs_and_has_actions() {
        let cmds = all_commands(true);
        assert!(cmds.iter().all(|(label, _)| !label.trim().is_empty()));
        let mut gos = 0;
        let (mut upd, mut open, mut theme, mut temp) = (false, false, false, false);
        for (_, cmd) in &cmds {
            match cmd {
                Cmd::Go(tab) => {
                    assert!(tab.visible(), "aba invisível não pode aparecer na paleta");
                    gos += 1;
                }
                Cmd::UpdateYtdlp => upd = true,
                Cmd::OpenDownloads => open = true,
                Cmd::ToggleTheme => theme = true,
                Cmd::ClearTemp => temp = true,
            }
        }
        assert!(gos > 0, "paleta deve navegar para abas");
        assert!(upd && open && theme && temp, "todas as ações devem estar na paleta");
    }
}
