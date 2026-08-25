//! Neural embedder tuning sweep: batch size x thread count.
//! Run: MODEL_DIR=<path> cargo run --release --features neural --example bench_neural
#![allow(dead_code, unused_imports)]
// The #[path] include below pulls the real embed module into this example
// crate; its `use crate::config::VectorConfig` resolves against this shim
// (field-for-field mirror of the real config types used by make_embedder,
// which the bench never calls).
mod config {
    #[derive(Default)]
    pub struct NeuralConfig {
        pub model_dir: String,
        pub max_tokens: usize,
    }
    #[derive(Default)]
    pub struct VectorConfig {
        pub enabled: bool,
        pub embedder: String,
        pub neural: NeuralConfig,
    }
}
#[cfg(feature = "neural")]
#[path = "../src/index/embed/mod.rs"]
mod embed;
#[cfg(feature = "neural")]
use crate::embed::Embedder;

#[cfg(feature = "neural")]
fn main() {
    use rayon::prelude::*;
    use std::time::Instant;

    let model_dir = std::env::var("MODEL_DIR").expect("MODEL_DIR env");
    let embedder = embed::neural::CandleEmbedder::new(
        std::path::Path::new(&model_dir),
        "minilm-l6-v2",
        256,
    )
    .expect("load embedder");

    // ~512-token knowledge-doc-shaped texts, 256 docs total per config run.
    let text = "The Weapon module owns firing behaviour heat accumulation spread curves. "
        .repeat(64);
    let docs: Vec<String> = (0..256).map(|_| text.clone()).collect();

    // Warmup.
    let _ = embedder.embed_batch(&docs[..32]).unwrap();

    for batch_size in [32usize, 64, 128] {
        let batches: Vec<Vec<String>> = docs
            .chunks(batch_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        let start = Instant::now();
        for batch in &batches {
            let _ = embedder.embed_batch(batch).unwrap();
        }
        let serial = start.elapsed();

        for threads in [4usize, 8, 16] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let start = Instant::now();
            pool.install(|| {
                let _: Vec<_> = batches
                    .par_iter()
                    .map(|batch| embedder.embed_batch(batch).unwrap())
                    .collect();
            });
            let par = start.elapsed();
            println!(
                "batch={batch_size:3} serial={serial:6.1?} par{threads:2}={par:6.1?} speedup={:.1}x",
                serial.as_secs_f64() / par.as_secs_f64()
            );
        }
    }
}

#[cfg(not(feature = "neural"))]
fn main() {
    eprintln!("build with --features neural");
}
