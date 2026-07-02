fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/LUMEN DOWNLOADER ICO.ico");
        let _ = res.compile();
    }
}
