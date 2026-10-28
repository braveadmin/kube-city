# Fonts

The fonts the browser view uses, built into the binary so the page needs no
Google Fonts: Barlow, Barlow Condensed and IBM Plex Mono, all under the SIL
Open Font License 1.1 (see the `LICENSE-*` files).

The files are the Latin subsets (U+0000-00FF and common punctuation) from
the Fontsource 5.3.0 packages, for example
https://cdn.jsdelivr.net/npm/@fontsource/barlow@5.3.0/files/barlow-latin-400-normal.woff2,
renamed to `<family>-<weight>.woff2`. Other characters fall back to system
fonts. `fonts.css` names every file; `--web` puts it into the page in place of
the template's Google Fonts link. To add a weight, download it the same way
and add a line to `fonts.css`.
