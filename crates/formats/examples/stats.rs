fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let mut c = std::collections::BTreeMap::<String, usize>::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "xnb") {
            continue;
        }
        let d = std::fs::read(&p).unwrap();
        *c.entry(format!("flags {:#x} platform {}", d[5], d[3] as char)).or_default() += 1;
    }
    println!("{c:?}");
    let w = formats::wld::Header::parse(&std::fs::read(std::env::args().nth(2).unwrap()).unwrap()).unwrap();
    let data = std::fs::read(std::env::args().nth(2).unwrap()).unwrap();
    let mut kinds = [0u64; 4];
    formats::wld::read_tiles(&data, &w, |_, _, n, t| {
        if t.liquid != 0 {
            kinds[t.liquid_kind as usize] += n as u64
        }
    })
    .unwrap();
    println!("v{} liquids by kind {kinds:?}", w.version);
}
