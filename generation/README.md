# weekly_challenges.py

Generates challenge-related images for the Glyphs and Alphabets discord server. Assumes a Linux environment, and invokes `pdftoppm`, `convert` (part of `imagemagick`), `lualatex` and `fc-match` (for local font information). 

All images should be stored in a folder named `Images`. For quick testing, copy `sample_images` to `Images`. The usage examples below use the sample images. The format of this directory is as follows:

* Glyph submissions should have the filenames `Glyph_1`, `Glyph_2`, etc (with any image file extension).
* Ambigram submissions are likewise `Ambi_1`, `Ambi_2`, etc.
* User profile pictures should have filenames matching the user's names and should be stored in a subdirectory `Images/pfp`. Note that LaTeX doesn't like the characters `&%$#{}~^\` in filenames, so for now these should simply be stripped from stored usernames.
* The winning submissions should be copied to files named `GlyphWinnerFirst`, `GlyphWinnerSecond`, `GlyphWinnerThird` for the glyph challenge, and likewise `AmbiWinnerFirst`, `AmbiWinnerSecond`, `AmbiWinnerThird` for the ambigram challenge (with the appropriate file extensions).

For the full list of available options, consult ```weekly_challenges.py -h``` which will give a list of subcommands and a brief description of their usage, and ```weekly_challenges.py <SUBCOMMAND> -h``` for detailed description of one particular subcommand.

The LaTeX code in this repo is coauthored by one `doggo`.

## Table of Contents
- [Example Usage](#example-usage)
- [Font Selection](#font-selection)


## Example Usage
The following examples cover all of the glyph challenge-related commands, and the equivalent ambigram challenge commands work identically (with `ambigram` in place of `glyph`).

```
>>> weekly_challenges.py glyph_announcement 🐱
```
Date and background colour (which is on a weekly cycle) are automatically determined. For this example character, the font `Noto Emoji` is automatically selected and will be used so long as it is present on the system (if you get an error with this particular character, it's likely that `Noto Color Emoji`, which is more widely installed but will not work here, is being chosen instead). The font and font size can be overriden with the `--font` and `--size` flags, in this subcommand and several others.
<div align=center><img src="https://github.com/sungodmoth/weekly_challenges/assets/128005279/0b4f65bd-26c7-4dbf-9133-c25dc66aecdf" width="600"/></div>

```
>>> weekly_challenges.py glyph_poll 🐱
```
Glyph submissions are pulled from the `Images` directory as explained above. The number of columns, in this and other grid-related subcommands, is determined automatically but can be overriden by `--cols`.
<div align=center><img src="https://github.com/sungodmoth/weekly_challenges/assets/128005279/c64cd8f7-11ef-482e-af40-78b39697999f" width="600"/></div>

```
>>> weekly_challenges.py glyph_first dogg 
>>> weekly_challenges.py glyph_second the_uwuji 
>>> weekly_challenges.py glyph_third nope
```
Winning submissions are pulled from the appropriately named files in `Images` as explained above, and user profile pictures are likewise pulled from the images in `Images/pfp` with the same filenames as the usernames you pass in.
<div align=center><img src="https://github.com/sungodmoth/weekly_challenges/assets/128005279/cad0da8e-2757-4cb6-9c06-bb0456d2cb1e" width="600" style="display: inline-block; margin: 0px auto" /></div>
<div align=center width="600"><img src="https://github.com/sungodmoth/weekly_challenges/assets/128005279/25b41401-176b-435b-98c4-0d81ecf82f65" width="50%" /><img src="https://github.com/sungodmoth/weekly_challenges/assets/128005279/8b32b52a-03be-4d6b-81b0-15130b6bb2eb" width="50%" /></div>

```
>>> weekly_challenges.py glyph_suggestions a 木 🐱 ꁱ 爨 ꖵ さ ক্ষ
```
Suggestions are separated by spaces, and each one has a font auto-selected. Overriding these fonts and their sizes is not yet supported.
<div align=center><img src="https://github.com/sungodmoth/weekly_challenges/assets/128005279/e5f0d8cd-caca-4d1f-827e-d2255d3710e8" width="600"/></div>

## Font Selection
Automatic font selection proceeds according to the file `font_data.json`, which contains a list of fonts. Each font must be present on the system (as a `.ttf` or `.otf` in the relevant system directories, not just within the LaTeX installation), so that the `fontconfig` tool `fc-match` can be used to automatically determine which glyphs it supports (the exception is STIX Two Text, the main font of the document, whose glyph support has been hardcoded for convenience as it is rarely present outside of LaTeX).

The order of fonts within the file is significant because each string will be displayed in the *first* font in the list which supports *all* of its Unicode characters.

There are also a few additional flags which can be attached to a font:
* `"load_as"` is used for internal LaTeX font loading and can be mostly ignored.
* `"excludes"` can be used to dictate that even if a font supports a glyph in a given Unicode range, it will not be used for it, meaning that font selection will proceed further down the list. For example, `BabelStone Han` is set as the preferred font for Han characters, but is not the preferred font for several other Unicode blocks for which it has partial or full support, including Japanese kana. Excluding these ranges allows us to keep `BabelStone Han` above all other Han fonts without also prioritising its other glyphs.
* `"size_percentage"` can be used if a font has consistently oversized or undersized glyphs that would otherwise necessitate frequent manual overrides. It is a simple multiplier, expressed as a percentage, that will be applied whenever the font is automatically selected by the font selection algorithm. Note that if the same font is selected manually, the multiplier will not be applied.
