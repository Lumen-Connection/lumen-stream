// Embute o ícone do logo no executável (Windows): mostrado no Explorer e na
// barra de tarefas. Usa o ICO "logo apenas".
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/LUMEN DOWNLOADER ICO.ico");
        let _ = res.compile();
    }
}
