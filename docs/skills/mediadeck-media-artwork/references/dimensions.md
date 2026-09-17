# Canvas and print dimensions

## Canonical profiles

All print profiles use 300 pixels per inch (PPI). Convert millimetres with:

```text
pixels = round(mm / 25.4 × 300)
```

| Profile | Trim size | Primary PNG | Ratio | Default safe inset |
| --- | ---: | ---: | ---: | ---: |
| `launcher_cover` | screen-only | `1518 × 2076 px` | `253:346` | `90 px` |
| `floppy_35_label` | `70 × 52 mm` | `827 × 614 px` | `35:26` | `3 mm` / `35 px` |
| `cd_jewel_front` | `120 × 120 mm` | `1417 × 1417 px` | `1:1` | `3 mm` / `35 px` |
| `dvd_case_front` | `130 × 183 mm` | `1535 × 2161 px` | `130:183` | `3 mm` / `35 px` |

`launcher_cover` is exactly six times the launcher art aperture
(`253 × 346 px`). The runtime may downsample it but must not change its ratio.

The floppy default matches Avery L7666 stock. Floppy labels vary; use the exact
manufacturer die line when the customer names a different product.

The DVD profile is the **front panel only**, not the back/spine/front wrap.

## Optional production bleed

The two primary outputs use trim dimensions. When a print shop requires bleed,
create an optional working/source file in addition to — never instead of — the
primary trimmed PNG.

| Profile | 3 mm bleed on every edge | Working canvas at 300 PPI |
| --- | ---: | ---: |
| `floppy_35_label` | `76 × 58 mm` | `898 × 685 px` |
| `cd_jewel_front` | `126 × 126 mm` | `1488 × 1488 px` |
| `dvd_case_front` | `136 × 189 mm` | `1606 × 2232 px` |

Extend background art through bleed. Keep titles, IDs and provider badges
inside the safe area. Do not bake trim guides or crop marks into the primary
PNG.

## Density metadata

For print outputs, encode 300 PPI when the export tool supports it. In PNG this
is approximately `11811` pixels per metre in both axes. Pixel count and
physical print settings both matter: print at `100%` or `actual size`, never
`fit to page`.

## Sources

- MediaDeck `PRODUCT_SPEC.md`, FR-060 through FR-067: millimetres, 300 PPI,
  safe area, bleed, and calibration requirements.
- MediaDeck `PHYSICAL_MEDIA_SPEC.md`, section 13: artwork presets and editable
  manufacturer dimensions.
- Avery L7666 3.5-inch diskette label: `70 × 52 mm`:
  https://www.avery.ae/template-l7666
- CD jewel front artwork: `120 × 120 mm`:
  https://www.blankmediaprinting.com/templates-download-area
- Standard DVD keep-case front panel: `130 × 183 mm`:
  https://www.stebbing.co.nz/application/files/4715/8328/8073/DVD_Spec_Booklet_2019_updated.pdf

These are baseline presets, not a claim that every label stock or case has
identical tolerances.
