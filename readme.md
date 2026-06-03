# NovaLauncher

A small Minecraft launcher written in Rust with an `iced` GUI and
`mc-launcher-core` for installation and launch command generation.

## Features

- Offline username launching
- Vanilla, Fabric, Quilt, Forge, and NeoForge loader choices
- Mojang Minecraft version list with release, snapshot, old beta, and old alpha filters
- Staged install, download, and startup progress bar
- Custom Java executable path, or `java` from `PATH`
- Memory slider
- Isolated launcher data directory

## Run

```powershell
cargo run
```

The first launch of a Minecraft version can take a while while assets,
libraries, natives, and client files are downloaded.
