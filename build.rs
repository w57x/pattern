use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let shader_dir = PathBuf::from("src/shaders");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=src/shaders");

    let entries = fs::read_dir(shader_dir).expect("Failed to read shader directory");

    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();

        if let Some(ext) = path.extension()
            && (ext == "vert" || ext == "frag")
        {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            let output_path = out_dir.join(format!("{}.spv", file_name));

            println!("cargo:warning=Compiling shader: {}", file_name);

            let status = Command::new("glslangValidator")
                .arg("-V")
                .arg(&path)
                .arg("-o")
                .arg(&output_path)
                .status()
                .expect("Failed to execute glslangValidator");

            if !status.success() {
                panic!("Shader compilation failed for {}", file_name);
            }
        }
    }
}
