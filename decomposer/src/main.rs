use std::env;
use wirm::{Component};

fn main() -> Result<(), std::io::Error> {
    let Some(app_wasm_path) = env::args().nth(1) else {
        panic!("usage: decomposer <composition.wasm>");
    };

    let buff = std::fs::read(app_wasm_path).unwrap();
    let mut component = Component::parse(&buff, false, false).expect("Unable to parse");

    for (i, internal_comp) in component.components.iter_mut().enumerate() {
        internal_comp.emit_wasm(&format!("split{i}.wasm"))?;
    }

    Ok(())
}