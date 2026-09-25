use anyhow::Result;

/// Prints the command overview shown when `mfp` runs without a subcommand.
pub(super) fn interactive_mode() -> Result<()> {
    println!("Music For Programming - Radio Player");
    println!("\nComandos disponibles:");
    println!("  mfp list                    - Lista todos los episodios");
    println!("  mfp play                    - Reproduce desde el inicio");
    println!("  mfp play -e 75              - Reproduce episodio específico");
    println!("  mfp play -s                 - Reproduce en modo shuffle");
    println!("  mfp play -f                 - Reproduce solo favoritos");
    println!("  mfp fav -l                  - Lista favoritos");
    println!("  mfp fav -a \"Episode XX\"     - Agrega a favoritos");
    println!("  mfp fav -r \"Episode XX\"     - Remueve de favoritos");
    println!("  mfp download -e 75          - Descarga episodio para offline");
    println!("  mfp download --list         - Lista episodios descargados");
    println!("\nUsa 'mfp play' para comenzar a escuchar");

    Ok(())
}
