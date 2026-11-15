# Tilezone

A Tile dashboard with a Liquid Glass aesthetic, written in Rust. Tiles are rearrangeable glass panels hosting small applets, drawn over a live wallpaper (still image, animated GIF or WebP, video, or fallback). Uses wgpu.

## Features

- Liquid Glass material
- Static, video or generated wallpapers
- Multi-applet tiles
- Edit mode for tiles
- Interactive and animated everything!
- Desktop pet cat!

## Requirements

- RUST 1.85 or newer
- A Vulkan, Metal, or DirectX 12 capable GPU
- `ffmpeg` on `PATH` for video wallpapers (optional)
- Linux: an XDG desktop portal implementation for file dialogs (present on GNOME and KDE)

## Build and run

```sh
cargo run --release
cargo run --release -- path/to/wallpaper.mp4
```

## Controls

| Input | Action |
| --- | --- |
| Hover tile, top right button | Expand to full size |
| Hover tile, chevron button | Select applet |
| Ctrl + Scroll over a tile | Cycle applets |
| Long press a tile | Enter edit mode |
| Edit mode: drag / corner handle / minus badge | Move or Merge / resize / remove |
| Edit mode: Add | Insert a new tile |
| Left click | Outward warp |
| Right click | Inward warp |
| Escape | Leave edit mode or collapse tile |

## Vendored dependency

`vendor/egui_glass` is a patched copy of `egui_glass` 0.2.0.

## Coming features

- Adding more applets, always
- Feature to rearrange applets within tiles
- Feature to remove an applet from a multi-applet tile.

## Credits

- egui and eframe by Emil Ernerfeldt and contributors
- egui_glass by heonny
- Inter by Rasmus Andersson
- JetBrains Mono by JetBrains
- Cat sprites by Elthen (https://elthen.itch.io/2d-pixel-art-cat-sprites)