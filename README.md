# Tilezone

A GPU-rendered tile dashboard with a Liquid Glass aesthetic, written in Rust. Tiles are rearrangeable glass panels hosting small applets, drawn over a live wallpaper (still image, animated GIF or WebP, video, or procedural aurora [fallback] ).

## Features

- Liquid Glass material: multi-tap blur, edge refraction, chromatic dispersion, specular rim, and backdrop magnification
- Live backdrop: expanded tiles refract the tiles beneath them, not just the wallpaper
- Animated wallpaper: image, GIF, WebP, video (via ffmpeg), or built-in procedural aurora
- Spring-driven layout: every tile move, resize, spawn, and removal is animated, including window resizes
- Multi-applet tiles: Ctrl+Scroll cycles applets within a tile, with a dropdown for manual selection
- Grid editor: drag, resize with snapping, add, and remove tiles
- Click warp: pointer clicks emit a refractive shockwave that bends the wallpaper and displaces tiles
- Desktop pet that walks, hops between tile edges, and sleeps when idle

## Applets

| Applet | Description |
| --- | --- |
| System | CPU, RAM, GPU (when available), and disk gauges |
| Reminders | Task list |
| Notes | Plain text notepad |
| Clipboard | Recent clipboard history |
| Photo | Pinned picture |
| Viewer | Folder-aware image viewer with zoom and pan |
| Paint | Antialiased canvas with undo and PNG export |
| Click speed | Clicks per second counter |
| Snake | Classic snake |
| Space shooter | 2D arcade shooter |
| Settings | Background selection, refraction, zoom, reduce effects, pet toggle |

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
| Edit mode: drag / corner handle / minus badge | Move / resize / remove |
| Edit mode: Add | Insert a new tile |
| Left click | Outward warp |
| Right click | Inward warp |
| Escape | Leave edit mode or collapse tile |

## Vendored dependency

`vendor/egui_glass` is a patched copy of `egui_glass` 0.2.0.

## Credits

- 
