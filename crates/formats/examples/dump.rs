fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    for name in &a[3..] {
        let Ok(data) = std::fs::read(dir.join(format!("{name}.xnb"))) else {
            println!("{name}: missing");
            continue;
        };
        let tex = formats::xnb::read_texture(&data).unwrap();
        let mut f = format!(
            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
            tex.width, tex.height
        )
        .into_bytes();
        f.extend_from_slice(&tex.rgba);
        std::fs::write(format!("{}/{name}.pam", a[2]), f).unwrap();
        println!("{name}: {}x{}", tex.width, tex.height);
    }
}
