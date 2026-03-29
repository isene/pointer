# Pointer

Rust feature clone of [RTFM](https://github.com/isene/RTFM), a terminal file manager.

Two-pane file manager with preview, image display, archive handling, SSH, and plugin support. Built on Crust.

## Build

```bash
PATH="/usr/bin:$PATH" cargo build --release
```

Note: `PATH` prefix needed to avoid `~/bin/cc` (Claude Code sessions) shadowing the C compiler.
