# glyfi - a bot for the Glyphs and Alphabets Discord server

Much of the code in this repo (see initial commit) was authored by @Sirraide, so big thanks to them!
The LaTeX code was co-written with one `doggo` on Discord.

Aside from its Rust dependencies, this bot relies on (Xe)LaTeX and the (Linux-oriented) command-line tools `convert` (part of `imagemagick`) and `pdf2ppm`; the `fontconfig` tool `fc-match` is also invoked to aid automatic font selection.

## Running
The first time you start the bot, or after adding a command, run
```bash
$ cargo run -- --register
```

From then on, just run

```bash
$ cargo run
```

Press CTRL+C to shut down the bot gracefully.