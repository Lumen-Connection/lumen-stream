fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/LumenStreamIcon.ico");
        res.compile()
            .expect("falha ao embutir o ícone no exe (assets/LumenStreamIcon.ico)");
    }
}
