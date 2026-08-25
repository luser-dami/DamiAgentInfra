#![allow(dead_code, unused_imports)]
//! Micro-benchmark: time HashNGramEmbedder on representative node texts.
use std::time::Instant;

#[path = "../src/index/embed/mod.rs"]
mod embed;

use crate::embed::Embedder;
use embed::HashNGramEmbedder;

fn main() {
    // Representative 4000-char node text (the compile-time cap).
    let text = "The Weapon module owns firing behaviour and heat accumulation. \
                USkillFragment is defined at src/math.ts:1 and carries the fragment base. \
                cooldown overheating spread curves recoil animation montage notify window "
        .repeat(36);
    println!("text chars: {}", text.chars().count());

    let embedder = HashNGramEmbedder::default();
    // Warmup
    let _ = embedder.embed(&text).unwrap();

    let start = Instant::now();
    let n = 4500;
    for _ in 0..n {
        let _ = embedder.embed(&text).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "{} embeds in {:?} => {:.2} ms/doc",
        n,
        elapsed,
        elapsed.as_secs_f64() * 1000.0 / n as f64
    );
}
