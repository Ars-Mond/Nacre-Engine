//! Minimal demo that links against the `nacre_engine` library crate.
//!
//! This scaffold only confirms that the library builds and links. A real demo
//! will own the window, event loop, and wgpu `Device`/`Queue`, then hand a
//! target texture to the engine — see Principle III in the project constitution.
//!
//! Run with: `cargo run --example demo`

fn main() {
    println!(
        "NacreEngine v{} — library crate linked successfully.",
        nacre_engine::VERSION
    );
}
