use cssh_rs_meta::PACKAGE_NAME;
use std::path::PathBuf;
use svg_to_ico::svg_to_ico;
extern crate embed_resource;

fn main() {
    let svg_path = PathBuf::from(format!("res/windows/{PACKAGE_NAME}.svg"));
    let ico_path = PathBuf::from(format!("res/{PACKAGE_NAME}.ico"));
    let rc_path = format!("res/{PACKAGE_NAME}.rc");
    println!("cargo:rerun-if-changed={}", svg_path.display());
    println!("cargo:rerun-if-changed={rc_path}");
    svg_to_ico(&svg_path, 96.0, &ico_path, &[16, 24, 32, 48, 64, 128, 256])
        .expect("Failed to convert SVG to ICO");
    embed_resource::compile_for(&rc_path, [PACKAGE_NAME], embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
