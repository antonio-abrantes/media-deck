# Need for Speed: Underground — validation test

Date: 2026-09-16  
Game: first `Need for Speed: Underground` (2003), not `Underground 2`  
Provider: `EA`  
Media ID: `NFSU-001`

## Results

| File | Final size | Capacity badge |
| --- | ---: | --- |
| `need-for-speed-underground-launcher-cover.png` | 1518 × 2076 px | — |
| `need-for-speed-underground-floppy-35-label.png` | 827 × 614 px; 70 × 52 mm | `COMPRESSED CORE: 1.44 MB` |
| `need-for-speed-underground-cd-jewel-front.png` | 1417 × 1417 px; 120 × 120 mm | `OPTICAL CORE: 700 MB` |

Every print file is a flat front-facing composition at 300 PPI. No file uses
`LOCAL INSTALLATION REQUIRED`.

## Final prompt set

```text
Create a matched MediaDeck set for the first Need for Speed Underground from
2003: orange early-2000s tuner coupe, wet underground city at night, neon
reflections and high-speed motion. Use a mandatory angular technical border
with nested keylines, circuit traces, nodes and dark separator. Derive the
border from the game palette: asphalt black and metallic silver, neon green as
the main highlight, electric blue and violet as sparse accents. Keep orange
mainly on the car. Recompose separately for the portrait launcher cover,
landscape floppy label and square jewel front. Render exact metadata and the
preset-specific CORE line. Never include the number 2, a woman, licensed car
logos, a media body/case, perspective mockup, ratings, barcode, crop marks,
watermark or extra text.
```

## Validation

Both cover+media pairs passed `validate_artwork.py` with `--strict-density`:

```text
OK: cover 1518x2076
OK: floppy_35_label 827x614
OK: cover 1518x2076
OK: cd_jewel_front 1417x1417
```

Visual reinspection confirmed the first-game identity, exact capacity lines,
complete technical borders, palette-derived accents, readable metadata and no
`Underground 2` elements.
