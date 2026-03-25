# resources/sfx/ — Licenses & Attribution

 SFX library — 6 curated + loudnorm'd WAV samples embedded via
`include_bytes!` in `src/brand/sound_sink.rs` and reused for promo MP4
audio bake (single source of truth per ).

| File | Source Library | License | Upstream URL |
|------|---------------|---------|--------------|
| hero.wav | Material Sound Resources | Apache-2.0 | https://material.io/design/sound/ |
| apply.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| success.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| failure.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| select.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| click.wav | Kenney UI Audio Pack | CC0 1.0 | https://kenney.nl/assets/ui-audio |

## Notices

Material Sound Resources (Apache-2.0): attribution preserved above; no
modifications to upstream sample except loudnorm normalization.

Octave UI Sounds (MIT): source MIT license reproduced in
`resources/sfx/README.md` curation notes.

Kenney UI Audio Pack (CC0 1.0): public domain, attribution given
as courtesy.

## Curation pipeline

All samples processed via `./scripts/curate-sfx.sh` with loudnorm target
-18 LUFS integrated, -1.5 dBTP, 11 LU LRA, 44.1kHz mono 16-bit.
Content integrity pinned via `resources/sfx/SHA256SUMS` — run
`shasum -a 256 -c SHA256SUMS` in this directory to re-verify.

## Scaffolding Note

 commits placeholder synthesized tones (produced via ffmpeg
`sine=` filter) to satisfy the `include_bytes!` compile target, size
budget, and format contract. The attribution table above corresponds to
the library-sourced samples that   substitutes in without
changing the filename contract.
