# resources/sfx — SFX Library

Six WAV samples mastered for slate's sound feedback. Each sample maps to a
`BrandEvent` category through `src/brand/sound_sink.rs`; the same files can be
used by the promo audio bake script so release assets and runtime behavior stay
in sync.

## Sample → Event Mapping

| File | BrandEvent | Duration | Library (see LICENSE.md) | Character |
|------|------------|----------|--------------------------|-----------|
| hero.wav | SetupComplete | ~330ms | Material Sound Resources | wizard completion swell |
| apply.wav | ApplyComplete | ~150ms | Octave UI Sounds | checkbox-style latch |
| success.wav | Success(*) | ~100ms | Octave UI Sounds | short positive blip |
| failure.wav | Failure(*) | ~120ms | Octave UI Sounds | dampened low blip |
| select.wav | Selection(PickerEnter) | ~80ms | Octave UI Sounds | crisp UI click |
| click.wav | Navigation(PickerMove) | ~50ms | Kenney UI Audio Pack | short debounced tick |

## Mastering Pipeline

Run `./scripts/curate-sfx.sh <raw-input-dir> resources/sfx/` to reproduce the
normalization pass.

Target: **-18 LUFS integrated / -1.5 dBTP / 11 LU LRA**.
Output format: **44.1kHz mono 16-bit PCM WAV**.

## Size Budget

Each sample should stay at or below **30KB**, with the total library below
**120KB**. This keeps the `include_bytes!` binary impact small enough for the
release binary size budget. Current tally:

- hero.wav: ~32 KB
- apply.wav: ~16 KB
- success.wav: ~12 KB
- failure.wav: ~12 KB
- select.wav: ~8 KB
- click.wav: ~8 KB
- **Total:** ~88 KB

## Integrity

`SHA256SUMS` pins content hashes. Re-verify at any time:

```sh
(cd resources/sfx && shasum -a 256 -c SHA256SUMS)
```

All 6 lines must end with `OK`.

## Attribution

See `LICENSE.md` for per-file source and license notices. Samples are modified
only by the normalization pass described above.
