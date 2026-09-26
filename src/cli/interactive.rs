use anyhow::Result;

/// Prints the command overview shown when `mfp` runs without a subcommand and
/// without a terminal (so audio never starts unattended in a pipe).
pub(super) fn interactive_mode() -> Result<()> {
    println!("Music For Programming - Radio Player");
    println!("\nComandos disponibles:");
    println!("  mfp list                    - Lista todos los episodios");
    println!("  mfp                         - Abre la TUI (igual que mfp play)");
    println!("  mfp play                    - Reproduce desde el inicio (TUI)");
    println!("  mfp play -e 75              - Reproduce episodio específico");
    println!("  mfp play -s                 - Reproduce en modo shuffle");
    println!("  mfp play -f                 - Reproduce solo favoritos");
    println!("  mfp play --compact          - TUI en diseño compacto");
    println!("  mfp play --plain            - Interfaz de texto (comandos + Enter)");
    println!("  mfp fav -l                  - Lista favoritos");
    println!("  mfp fav -a \"Episode XX\"     - Agrega a favoritos");
    println!("  mfp fav -r \"Episode XX\"     - Remueve de favoritos");
    println!("  mfp download -e 75          - Descarga episodio para offline");
    println!("  mfp download --list         - Lista episodios descargados");
    println!("\nUsa 'mfp' o 'mfp play' en una terminal para comenzar a escuchar");
    println!("Sin terminal (pipe, TERM=dumb) 'mfp play' usa la interfaz de texto");

    Ok(())
}
