# NøteCraft patch of winit 0.30.13

Upstream: https://github.com/rust-windowing/winit (Apache-2.0, see LICENSE).

One change in `src/platform_impl/macos/view.rs`: `insertText:replacementRange:` ignored text that did not
come from a key press and had no marked text, so the macOS emoji and symbol picker (Control Command Space)
and dictation inserted nothing. That text is now sent as `Ime::Commit`. Text typed on the keyboard is
unchanged (a `in_key_down` flag tells the two apart).

A second change in `src/platform_impl/macos/menu.rs`: the default menu bar gets an Edit menu with the single item
"Emoji & Symbols" (`orderFrontCharacterPalette:`), which the keyboard's emoji key needs to open the picker. It has no
key equivalents, so editing shortcuts are unchanged.

Remove this folder and the `[patch.crates-io]` entry of the root `Cargo.toml` once a winit release fixes it.
