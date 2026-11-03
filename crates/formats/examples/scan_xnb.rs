fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let out = std::env::args().nth(2);
    let (mut ok, mut fail) = (0, 0);
    let mut errs: std::collections::BTreeMap<String, usize> = Default::default();
    let mut first: std::collections::BTreeMap<String, String> = Default::default();
    let t = std::time::Instant::now();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "xnb") {
            continue;
        }
        let data = std::fs::read(&p).unwrap();
        match formats::xnb::read_texture(&data) {
            Ok(tex) => {
                ok += 1;
                if let Some(o) = &out {
                    let name = p.file_stem().unwrap().to_string_lossy().to_string();
                    if [
                        "Tiles_0",
                        "Tiles_1",
                        "Wall_1",
                        "Liquid_0",
                        "Liquid_1",
                        "Player_0_0",
                        "NPC_1",
                        "Item_1",
                        "Tiles_4",
                    ]
                    .contains(&name.as_str())
                    {
                        let mut f = format!(
                            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
                            tex.width, tex.height
                        )
                        .into_bytes();
                        f.extend_from_slice(&tex.rgba);
                        std::fs::write(format!("{o}/{name}.pam"), f).unwrap();
                        println!("{name}: {}x{}", tex.width, tex.height);
                    }
                }
            }
            Err(e) => {
                fail += 1;
                let k = e.to_string();
                let k = k.split(':').take(2).collect::<Vec<_>>().join(":");
                first.entry(k.clone()).or_insert(p.file_name().unwrap().to_string_lossy().to_string());
                *errs.entry(k).or_default() += 1;
            }
        }
    }
    println!("ok {ok} fail {fail} in {:.2?}", t.elapsed());
    for (k, n) in errs {
        println!("{n:5} {k}  (e.g. {})", first[&k]);
    }
}
