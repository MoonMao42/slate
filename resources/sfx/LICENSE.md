# resources/sfx — Licenses & Attribution

The SFX library contains 6 curated WAV samples embedded through
`include_bytes!` in `src/brand/sound_sink.rs`.

| File | Source Library | License | Upstream URL |
|------|----------------|---------|--------------|
| hero.wav | Material Sound Resources | Apache-2.0 | https://material.io/design/sound/ |
| apply.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| success.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| failure.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| select.wav | Octave UI Sounds | MIT | https://mhmohona.gitbook.io/octave-ui-sounds |
| click.wav | Kenney UI Audio Pack | CC0 1.0 | https://kenney.nl/assets/ui-audio |

## Notices

Material Sound Resources (Apache-2.0): attribution preserved above; no
modifications to the upstream sample except loudness normalization.

Octave UI Sounds (MIT): attribution preserved above; samples are used under
the upstream MIT license.

Kenney UI Audio Pack (CC0 1.0): public domain; attribution given as a courtesy.

## Curation Pipeline

All samples were processed via `./scripts/curate-sfx.sh` with loudnorm target
`-18 LUFS integrated`, `-1.5 dBTP`, `11 LU LRA`, `44.1kHz mono 16-bit`.

Content integrity is pinned in `resources/sfx/SHA256SUMS`. Run
`shasum -a 256 -c SHA256SUMS` in this directory to verify the checked-in files.
