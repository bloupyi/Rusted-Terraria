fn main() {
    let path = std::env::args().nth(1).expect("usage: load_world <file.wld>");
    let start = std::time::Instant::now();
    let w = world::World::load(path.as_ref()).unwrap();
    let parsed = start.elapsed();
    let (t, wl) = w.used_types();
    let typed = start.elapsed();
    world::light::compute(&w);
    println!(
        "{}x{} parsed {parsed:.2?}, {} tile types, {} wall types, types {:.2?}, light {:.2?}",
        w.width,
        w.height,
        t.len(),
        wl.len(),
        typed - parsed,
        start.elapsed() - typed
    );
}
