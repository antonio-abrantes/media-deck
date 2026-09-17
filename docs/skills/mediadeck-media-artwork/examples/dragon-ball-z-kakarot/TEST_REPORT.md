# Dragon Ball Z: Kakarot — validated example set

Date: 2026-09-16  
Provider: `STEAM`  
Steam AppID: `851850`  
Media ID: `GAME-001`

## Results

The universal launcher cover is paired with all three physical-media presets:

| File | Final size | Capacity badge |
| --- | ---: | --- |
| `dragon-ball-z-kakarot-launcher-cover.png` | 1518 × 2076 px | — |
| `dragon-ball-z-kakarot-floppy-35-label.png` | 827 × 614 px; 70 × 52 mm | `COMPRESSED CORE: 1.44 MB` |
| `dragon-ball-z-kakarot-cd-jewel-front.png` | 1417 × 1417 px; 120 × 120 mm | `OPTICAL CORE: 700 MB` |
| `dragon-ball-z-kakarot-dvd-case-front.png` | 1535 × 2161 px; 130 × 183 mm | `OPTICAL CORE: 4.70 GB` |

Every print file is a flat front-facing composition at 300 PPI. No file uses
`LOCAL INSTALLATION REQUIRED`.

## Art direction prompt

```text
Match the existing Kakarot launcher identity: Goku in orange, golden energy,
sunset mountains, navy shadows, red/orange technical frame with golden and cyan
accents. Recompose independently for each target ratio. Use a complete angular
MediaDeck border with nested keylines, circuit traces, nodes and a dark inner
separator. Keep metadata in a near-black lower band and render the exact
preset-specific CORE line. Flat artwork only; no media body, case, mockup,
perspective, crop marks, ratings, barcode, watermark or extra text.
```

## Validation

All three cover+media pairs passed `validate_artwork.py` with
`--strict-density`:

```text
OK: cover 1518x2076
OK: floppy_35_label 827x614
OK: cover 1518x2076
OK: cd_jewel_front 1417x1417
OK: cover 1518x2076
OK: dvd_case_front 1535x2161
```

The normalized files were visually reinspected. Titles, capacity badges,
borders, nodes, faces and safe-area content remain complete.
