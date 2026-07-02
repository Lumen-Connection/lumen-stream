
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
    vec![
        go("🏠", if pt { "Início" } else { "Home" }, Tab::Home),
        go("🎵", if pt { "Baixar Música" } else { "Download Music" }, Tab::Music),
        go("🎬", if pt { "Baixar Vídeo" } else { "Download Video" }, Tab::Video),
        go("🔄", "Lumen Converter", Tab::Converter),
        go("📋", if pt { "Fila" } else { "Queue" }, Tab::Queue),
        go("📁", if pt { "Pastas" } else { "Folders" }, Tab::Folders),
        go("🖼", if pt { "Galeria" } else { "Gallery" }, Tab::Gallery),
        go("☁", if pt { "Nuvem" } else { "Cloud" }, Tab::Cloud),
        go("📊", if pt { "Estatísticas" } else { "Statistics" }, Tab::Stats),
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
    ]
}

pub fn run(app: &mut App, cmd: Cmd) {
    let pt = app.config.lang == crate::ui::i18n::Lang::Pt;
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
        Cmd::ClearTemp => {
            let n = app.clear_temp_files();
            app.toast(
                if pt {
                    format!("{} arquivo(s) temporário(s) removido(s)", n)
                } else {
                    format!("{} temp file(s) removed", n)
                },
                false,
            );
        }
    }
}
