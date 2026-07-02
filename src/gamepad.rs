/// Ações de navegação derivadas do controle (DualSense e compatíveis).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavAction {
    FocusNext,
    FocusPrev,
    ArrowLeft,
    ArrowRight,
    Activate,
    Back,
    NextTab,
    PrevTab,
    PlayPause,
    Stop,
    Palette,
    ToggleMode,
}

/// AppID do jogo da Steam em execução (0 = nenhum). Usado para ligar o modo
/// controle automaticamente quando o usuário entra num jogo.
#[cfg(windows)]
pub fn steam_running_appid() -> Option<u32> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let steam = hkcu.open_subkey(r"Software\Valve\Steam").ok()?;
    steam.get_value::<u32, _>("RunningAppID").ok()
}

#[cfg(not(windows))]
pub fn steam_running_appid() -> Option<u32> {
    None
}

/// True se há um jogo da Steam rodando agora.
pub fn steam_in_game() -> bool {
    steam_running_appid().map_or(false, |id| id != 0)
}

/// Encapsula o gilrs e traduz botões do controle em `NavAction`s.
pub struct GamepadNav {
    gilrs: Option<gilrs::Gilrs>,
    pub connected: bool,
    pub name: String,
}

impl Default for GamepadNav {
    fn default() -> Self {
        let gilrs = gilrs::Gilrs::new().ok();
        let (connected, name) = match &gilrs {
            Some(g) => {
                let mut it = g.gamepads();
                match it.next() {
                    Some((_, gp)) => (true, gp.name().to_string()),
                    None => (false, String::new()),
                }
            }
            None => (false, String::new()),
        };
        Self {
            gilrs,
            connected,
            name,
        }
    }
}

impl GamepadNav {
    /// Consome os eventos pendentes do controle e devolve as ações.
    pub fn poll(&mut self) -> Vec<NavAction> {
        use gilrs::EventType;
        let mut actions = Vec::new();
        let Some(gilrs) = &mut self.gilrs else {
            return actions;
        };
        while let Some(ev) = gilrs.next_event() {
            match ev.event {
                EventType::Connected => {
                    self.connected = true;
                    if let Some(gp) = gilrs.connected_gamepad(ev.id) {
                        self.name = gp.name().to_string();
                    }
                }
                EventType::Disconnected => {
                    self.connected = gilrs.gamepads().next().is_some();
                }
                EventType::ButtonPressed(btn, _) => {
                    if let Some(a) = map_button(btn) {
                        actions.push(a);
                    }
                }
                _ => {}
            }
        }
        actions
    }
}

fn map_button(btn: gilrs::Button) -> Option<NavAction> {
    use gilrs::Button;
    Some(match btn {
        // D-pad: foco (cima/baixo) e setas (esquerda/direita).
        Button::DPadDown => NavAction::FocusNext,
        Button::DPadUp => NavAction::FocusPrev,
        Button::DPadLeft => NavAction::ArrowLeft,
        Button::DPadRight => NavAction::ArrowRight,
        // Botões de face (DualSense): X confirma, O volta.
        Button::South => NavAction::Activate,
        Button::East => NavAction::Back,
        Button::North => NavAction::PlayPause,
        Button::West => NavAction::Stop,
        // Gatilhos de ombro trocam de aba.
        Button::RightTrigger => NavAction::NextTab,
        Button::LeftTrigger => NavAction::PrevTab,
        // Options abre a paleta de comandos.
        Button::Start => NavAction::Palette,
        // Botão PlayStation / Guia liga/desliga o modo controle.
        Button::Mode => NavAction::ToggleMode,
        _ => return None,
    })
}
