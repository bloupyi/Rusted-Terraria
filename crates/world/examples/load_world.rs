use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("usage: load_world <file.wld>");
    let start = Instant::now();
    let mut w = world::World::load(path.as_ref()).unwrap();
    let parsed = start.elapsed();
    let (t, wl) = w.used_types();
    let typed = start.elapsed();
    world::light::LightMap::compute(&w);
    println!(
        "{}x{} parsed {parsed:.2?}, {} tile types, {} wall types, types {:.2?}, light {:.2?}",
        w.width,
        w.height,
        t.len(),
        wl.len(),
        typed - parsed,
        start.elapsed() - typed
    );
    let mut sim = world::liquid::LiquidSim::new(&w);
    for i in 0..3000 {
        let (active, t) = (sim.active(), Instant::now());
        sim.tick(&mut w);
        w.take_dirty();
        if i % 500 == 0 {
            println!("liquid tick {i}: {active} active chunks, {:.2?}", t.elapsed());
        }
    }
}
