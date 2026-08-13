use std::{env, fs, sync::Arc};

use wasmppt_opc::ZipArchive;
use wasmppt_template::{CompilerOptions, InjectionData, PreparedTemplate, TemplateCompiler};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let template = env::args_os()
        .nth(1)
        .ok_or("usage: compile_generate TEMPLATE OUTPUT [BINDING VALUE]")?;
    let output = env::args_os()
        .nth(2)
        .ok_or("usage: compile_generate TEMPLATE OUTPUT [BINDING VALUE]")?;
    let binding = env::args().nth(3);
    let value = env::args().nth(4);
    if binding.is_some() != value.is_some() {
        return Err("binding and value must be supplied together".into());
    }

    let bytes: Arc<[u8]> = fs::read(template)?.into();
    let archive = ZipArchive::from_bytes(bytes.clone())?;
    let compiled = TemplateCompiler::new(CompilerOptions::default()).compile(&archive)?;
    for diagnostic in &compiled.diagnostics {
        eprintln!("{:?}: {}", diagnostic.code, diagnostic.message);
    }
    let prepared = PreparedTemplate::new(bytes, compiled.plan)?;
    let data = match (binding, value) {
        (Some(binding), Some(value)) => InjectionData::new().with_text(binding, value),
        _ => InjectionData::new(),
    };
    let generated = prepared.generate(&data)?;
    fs::write(output, generated.bytes)?;
    println!(
        "generated {} entries ({} rewritten entries)",
        generated.zip_stats.entries, generated.rewritten_entries
    );
    Ok(())
}
