# chessMarkable RPP

[![rMPP](https://img.shields.io/badge/rMPP-supported-green)](https://remarkable.com/store/remarkable-paper-pro)
[![AppLoad](https://img.shields.io/badge/AppLoad-compatible-blue)](https://github.com/pFeurle/AppLoad)

A chess game for the **reMarkable Paper Pro**, ported from [LinusCDE/chessmarkable](https://github.com/LinusCDE/chessmarkable). Features color board rendering on the Gallery 3 e-ink display, pawn promotion UI, and PGN viewer.

This port replaces the original `libremarkable` framebuffer backend with [QTFB](https://github.com/AnotherStranger/qtfb-client) (shared-memory display protocol) and swaps `pleco` (x86-only) for [shakmaty](https://crates.io/crates/shakmaty) (pure Rust, aarch64 compatible).

## Features

- **Color chess board** — warm tan/brown squares using the RPP's Gallery 3 color e-ink
- **Pawn promotion UI** — modal overlay to choose Queen, Rook, Bishop, or Knight
- **Player vs Player** with optional board rotation
- **Player vs Bot** at Easy, Normal, and Hard difficulty
- **PGN Viewer** — load and step through PGN files
- **Save/resume** — game state persisted across sessions
- **Streak-free display** — QTFB handles proper e-ink waveform selection (GC16/DU)

## Controlling

Move a chess piece by either:

1. Tap it once, then tap the destination square
2. Tap and drag to the destination square (doesn't show move hints)

## Installation via AppLoad

1. Copy the `appload/` directory contents to `/home/root/xovi/exthome/appload/chessmarkable/` on the device
2. Copy the built binary as `chessmarkable` into that directory
3. Copy `icon.png` into that directory
4. Restart AppLoad — the app should appear in the launcher

The directory should contain:
```
/home/root/xovi/exthome/appload/chessmarkable/
  chessmarkable            # binary
  icon.png                 # app icon
  external.manifest.json   # AppLoad manifest
```

## Building from Source

Requires [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) for cross-compilation:

```bash
cargo install cargo-zigbuild
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

The resulting static binary is at `target/aarch64-unknown-linux-musl/release/chessmarkable`.

Deploy to the device:

```bash
scp target/aarch64-unknown-linux-musl/release/chessmarkable \
    root@<device-ip>:/home/root/xovi/exthome/appload/chessmarkable/chessmarkable
```

## PGN Viewer

Place PGN files in `~/.config/chessmarkable/pgn/` on the device. Browse and step through games from the "PGN Viewer" menu.

## FEN Debugging

Set `RUST_LOG=debug` to print FEN notation on each move. Game state is saved to `~/.config/chessmarkable/savestates.yml`.

## Key Changes from Original

| Area | Original (rM1/rM2) | This Port (rMPP) |
|------|-------------------|-------------------|
| Display | `libremarkable` framebuffer | QTFB shared-memory protocol |
| Chess engine | `pleco` (x86 Stockfish port) | `shakmaty` (pure Rust) |
| Architecture | armv7 (gnueabihf) | aarch64 (musl, static) |
| Colors | Grayscale only | RGB color board |
| Pawn promotion | Auto-promoted to knight | Interactive piece selection |
| Launcher | Toltec/Oxide | AppLoad |

## Credit

- [LinusCDE/chessmarkable](https://github.com/LinusCDE/chessmarkable) — original reMarkable chess game
- [shakmaty](https://crates.io/crates/shakmaty) — chess move generation and validation
- [chess_pgn_parser](https://crates.io/crates/chess_pgn_parser) — PGN parsing
- Chess pieces from [Pixabay](https://pixabay.com/vectors/chess-pieces-set-symbols-game-26774/)
- QTFB protocol documentation from [AnotherStranger/qtfb-client](https://github.com/AnotherStranger/qtfb-client)
