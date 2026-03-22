# resources/sfx — SFX Library

Six curated WAV samples mastered for slate's  sound design. Each sample
maps to one `BrandEvent` variant via the `SoundSink` dispatch router
(`src/brand/sound_sink.rs`); the same samples power the promo MP4 audio bake
(`scripts/bake-audio.sh`) so users hear identical sound on first-run as they
did in the README preview ( single-source-of-truth).

Traceability:  CONTEXT  (6-sample mapping) (hero distinct) +
`` (sonic direction — Raycast / Things 3 /
Linear / PS5 chord swell coordinates).

## Sample → Event Mapping

| File         | BrandEvent             | Duration | Library (see LICENSE.md) | Character                        |
|--------------|------------------------|----------|--------------------------|----------------------------------|
| hero.wav     | SetupComplete          | ~330ms   | Material Sound Resources | hero chord swell (wizard finish) |
| apply.wav    | ApplyComplete          | ~150ms   | Octave UI Sounds         | checkbox-style latch             |
| success.wav  | Success(*)             | ~100ms   | Octave UI Sounds         | Linear-style sine blip           |
| failure.wav  | Failure(*)             | ~120ms   | Octave UI Sounds         | dampened low blip                |
| select.wav   | Selection(PickerEnter) | ~80ms    | Octave UI Sounds         | crisp UI click                   |
| click.wav    | Navigation(PickerMove) | ~50ms    | Kenney UI Audio Pack     | short tick (50ms-debounced)      |

## Mastering Pipeline

Run `./scripts/curate-sfx.sh <raw-input-dir> resources/sfx/` to reproduce.

Loudnorm target: **-18 LUFS integrated / -1.5 dBTP / 11 LU LRA**. Output format:
**44.1kHz mono 16-bit PCM WAV**.

## Size Budget

Per RESEARCH §10: each sample **≤ 30KB**, total **≤ 120KB**. Keeps the binary
diff from `include_bytes!` within `brew install` acceptance headroom. Current
tally (run `du -k resources/sfx/*.wav` to verify):

- hero.wav: 28.5 KB
- apply.wav: 13.0 KB
- success.wav: 8.7 KB
- failure.wav: 10.4 KB
- select.wav: 7.0 KB
- click.wav: 4.4 KB
- **Total:** ~72 KB (under 120KB ceiling)

## Integrity

`SHA256SUMS` pins content hashes. Re-verify at any time:

```
(cd resources/sfx && shasum -a 256 -c SHA256SUMS)
```

All 6 lines must end `OK`.

## Attribution

See `LICENSE.md` for per-file source + license notices. The Material Sound
Resources samples are Apache-2.0-licensed; Octave UI Sounds is MIT; Kenney UI
Audio Pack is CC0 1.0. No upstream sample is modified beyond the loudnorm
normalization pass described above.

---

## Scaffolding Note

The samples committed in  are **placeholder synthesized tones**
produced by the ffmpeg `sine=frequency=...` filter at loudnorm-comparable
volume. They establish the `include_bytes!` compile target, the size budget,
and the file-format contract so  Task 3 (`src/brand/sound_sink.rs`
skeleton) compiles.   replaces them with the library-sourced
curated samples listed in the mapping table above, with the curation pipeline
and attribution preserved byte-for-byte.
