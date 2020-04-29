//! If you see this, that means you are reading the source code for this package,
//! and maybe compiling it yourself
//!
//! This binary is just for my personal testing and is not intended for consumer use

use ppm::*;
use std::collections::HashMap;

fn main() {
    let s = "%alt(:::a)";
    let vars = HashMap::new();
    let mut en = Engine::with_predefined_commands(vars);
    let st = en.process_new(s.to_string());
    println!("{:#?}", st);
}
