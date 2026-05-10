fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon/launcher_icon.ico");
    res.compile().unwrap();
}
