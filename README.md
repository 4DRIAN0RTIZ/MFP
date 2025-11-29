# MFP - Music For Programming Radio Player

Reproductor ligero en Rust para la radio [Music For Programming](https://musicforprogramming.net/).

## Características

- **Streaming progresivo real** - Comienza a reproducir después de solo 512KB de buffer
- **Barra de progreso interactiva** - Visualiza tiempo transcurrido, restante y porcentaje de reproducción
- **Controles completos de reproducción** - Pausa/resume, volumen (+/-), silenciar (m), información (i)
- **Sistema de descargas offline** - Descarga episodios para escuchar sin conexión
- Sistema de favoritos persistente
- Modo shuffle
- Interfaz CLI simple y rápida
- **Controles interactivos sin bloqueos** - Navegación instantánea entre episodios
- Binario optimizado y ligero (3.6 MB)
- **Reproducción de audio de bajo nivel** - Sin dependencias externas (no requiere mpv, ffmpeg, etc.)
- Decodificación nativa de MP3, FLAC, WAV, Vorbis, AAC, ALAC y más formatos
- **Descarga en background** - El audio se descarga mientras se reproduce

## Instalación

```bash
cargo build --release
```

El binario optimizado estará en `target/release/mfp`.

### Instalación opcional en el sistema

```bash
sudo cp target/release/mfp /usr/local/bin/
```

## Uso

### Listar episodios
```bash
mfp list
```

### Reproducir
```bash
# Desde el primer episodio
mfp play

# Episodio específico
mfp play -e 75

# Con shuffle
mfp play -s

# Solo favoritos
mfp play -f

# Favoritos con shuffle
mfp play -f -s
```

### Gestionar favoritos
```bash
# Listar favoritos
mfp fav -l

# Agregar favorito
mfp fav -a "Episode 75: Datassette"

# Remover favorito
mfp fav -r "Episode 75: Datassette"
```

### Gestionar descargas offline
```bash
# Descargar un episodio específico
mfp download -e 75

# Listar episodios descargados
mfp download --list

# Ver espacio usado
mfp download --size

# Eliminar un episodio descargado
mfp download --delete "Episode 75"
```

Los episodios se descargan a `~/.config/mfp/downloads/`

## Controles durante reproducción

Durante la reproducción verás una barra de progreso interactiva:
```
[03:45/58:23] ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 6% | -54:38 >
```

Controles disponibles:
- `n` o `next` - Siguiente episodio
- `b` o `back` - Episodio anterior
- `p` o `pause` - Pausar/reanudar reproducción
- `+` o `up` - Aumentar volumen
- `-` o `down` - Disminuir volumen
- `m` o `mute` - Silenciar/desilenciar
- `i` o `info` - Mostrar información del episodio actual
- `s` o `shuffle` - Toggle shuffle
- `f` o `favorite` - Toggle favorito del episodio actual
- `d` o `download` - Descargar episodio actual para offline
- `q` o `quit` - Salir

## Arquitectura

El proyecto está organizado en módulos:

- `feed.rs` - Parser del RSS feed
- `player.rs` - Motor de streaming y reproducción de audio de bajo nivel
- `playlist.rs` - Gestión de playlist y shuffle
- `favorites.rs` - Sistema de favoritos persistente
- `downloader.rs` - Sistema de descargas offline
- `main.rs` - CLI y lógica principal

### Sistema de Streaming Progresivo

El reproductor utiliza una arquitectura de **threads separados** para streaming eficiente:

```
┌─────────────────────────────────────────────────────┐
│                   Thread Principal                   │
│                 (Interfaz de Usuario)                │
└─────────────────────────────────────────────────────┘
                          │
                          ├──► Control de navegación (n/p/s/f/q)
                          │
         ┌────────────────┴────────────────┐
         │                                  │
┌────────▼─────────┐              ┌────────▼──────────┐
│ Thread Descarga  │              │ Thread Reproducción│
│                  │              │                    │
│ • Descarga chunks│──── Canal ──►│ • Buffer 512KB    │
│   de 32KB        │   (mpsc)     │ • Decodificación  │
│ • HTTP streaming │              │ • Rodio playback  │
│ • Sin bloqueos   │              │                   │
└──────────────────┘              └───────────────────┘
```

**Características clave:**
- **Buffer inicial**: 512KB (~1-2 segundos de espera)
- **Chunks**: Descarga en bloques de 32KB
- **Cancelación rápida**: Los threads se detienen sin bloquear
- **Memoria eficiente**: Streaming continuo, no carga todo el archivo

## Dependencias principales

- `rodio` + `symphonia` - Reproducción y decodificación de audio de bajo nivel (Rust puro)
- `reqwest` - Cliente HTTP para obtener el RSS feed y streams de audio
- `rss` - Parser del feed XML
- `clap` - Framework para CLI con argumentos
- `serde` + `serde_json` - Serialización de favoritos
- `anyhow` - Manejo de errores mejorado
- `rand` - Generación aleatoria para shuffle
- `dirs` - Rutas de configuración del sistema

## Configuración

- Favoritos: `~/.config/mfp/favorites.json`
- Descargas offline: `~/.config/mfp/downloads/`

## Optimizaciones de compilación

El proyecto utiliza optimizaciones agresivas en modo release:

```toml
[profile.release]
opt-level = "z"      # Optimizar para tamaño
lto = true           # Link-Time Optimization
codegen-units = 1    # Mejor optimización
strip = true         # Eliminar símbolos de debug
```

Esto resulta en un binario muy pequeño y eficiente.

## Cómo funciona

### Flujo de reproducción

1. **Obtención de episodios**: Se descarga y parsea el RSS feed de musicforprogramming.net

2. **Gestión de playlist**: Los episodios se organizan en una lista que puede ser en orden o aleatoria

3. **Streaming progresivo de audio**:
   ```
   Usuario presiona Play
   ↓
   📡 Conectando... (Thread de descarga inicia)
   ↓
   ⏳ Buffering... (Acumula 512KB inicial)
   ↓
   ✓ (Decodifica MP3/FLAC/etc con Symphonia)
   ↓
   ▶️ Reproducción inicia (Thread de reproducción)
   ↓
   🎵 Audio se reproduce mientras continúa descargando en background
   ```

4. **Sistema de cancelación sin bloqueos**:
   - Cuando presionas `n` (next), el sink de audio se detiene instantáneamente
   - Los threads de descarga y reproducción terminan automáticamente
   - No hay esperas ni bloqueos - navegación inmediata

5. **Persistencia**: Los favoritos se guardan en formato JSON en `~/.config/mfp/favorites.json`

6. **Interactividad**: El programa lee comandos del usuario en tiempo real sin interferir con la reproducción

## Solución de problemas

### El comando mfp no se encuentra
Asegúrate de que el binario esté en tu PATH o usa la ruta completa: `./target/release/mfp`

### Error: "No se pudo inicializar el dispositivo de audio"
- Verifica que tu sistema tenga un dispositivo de audio configurado
- En Linux, asegúrate de que ALSA o PulseAudio estén funcionando
- Revisa los permisos de acceso al dispositivo de audio

### Sin audio durante reproducción
- Verifica el volumen de tu sistema
- Comprueba que el dispositivo de audio correcto esté seleccionado
- En Linux: Verifica que ALSA/PulseAudio estén configurados correctamente
- Revisa la configuración de audio de tu sistema

### La navegación (n/p) se siente lenta
- Esto es normal en conexiones lentas durante el buffering inicial
- El sistema espera 512KB antes de comenzar a reproducir
- Una vez iniciada la reproducción, la navegación es instantánea

### Error de red o descarga interrumpida
- El reproductor maneja automáticamente errores de red
- Si la descarga falla, simplemente presiona `n` para siguiente episodio
- Los threads se limpian automáticamente sin dejar recursos colgados

## Tecnologías utilizadas

- **[Rust](https://www.rust-lang.org/)** - Lenguaje de programación
- **[Rodio](https://github.com/RustAudio/rodio)** - Biblioteca de audio de alto nivel
- **[Symphonia](https://github.com/pdeljanov/Symphonia)** - Decodificador de audio puro en Rust
- **[Reqwest](https://github.com/seanmonstar/reqwest)** - Cliente HTTP para streaming
- **[Clap](https://github.com/clap-rs/clap)** - Parser de argumentos CLI

## Recursos

- [Music For Programming](https://musicforprogramming.net/) - Sitio oficial de la radio
- [RSS Feed](https://musicforprogramming.net/rss.xml) - Feed utilizado por el reproductor

## Rendimiento

- **Binario**: 3.6 MB (release optimizado)
- **Memoria**: ~10-20 MB durante reproducción (buffer de streaming)
- **Inicio**: ~1-2 segundos (buffering inicial de 512KB)
- **CPU**: Bajo consumo (~2-5% en sistemas modernos)
- **Red**: Descarga progresiva, no requiere descargar el archivo completo

## Licencia

GNU General Public License v3.0
