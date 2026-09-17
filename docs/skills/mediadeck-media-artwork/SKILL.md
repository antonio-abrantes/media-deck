---
name: mediadeck-media-artwork
description: "Create a matched pair of MediaDeck artwork files: one fixed-size launcher cover and one exact-size printable label or front insert for 3.5-inch floppy, CD jewel case, or DVD keep case. Use when a user asks for game cover art, physical-media labels, printable MediaDeck packaging, or a two-image MediaDeck artwork set from web or supplied references."
---

# MediaDeck Media Artwork

Create artwork as a standalone customer tool. Do not integrate generation into
the MediaDeck application or change app runtime code unless the user separately
requests that work.

## Required inputs

Collect or infer only these fields:

- game title;
- media preset: `floppy_35_label`, `cd_jewel_front`, or `dvd_case_front`;
- provider display name, such as `STEAM` or `LOCAL`;
- MediaDeck media ID when available;
- provider game ID when useful, such as a Steam AppID;
- supplied images and whether each is content, character, logo, or style
  reference;
- optional art direction.

Ask a question only when the game title or media preset is missing. Do not ask
for executable paths, process names, command lines, credentials, or data that
does not belong on artwork.

## Read before generating

Always read:

- `references/dimensions.md` for exact canvases and print rules;
- `references/art-direction.md` for hierarchy and the MediaDeck visual system;
- `references/provenance-and-rights.md` before web image research or reuse.
- `references/export-normalization.md` when the generator does not emit the
  exact requested pixel canvas.

Use the matching file from `assets/templates/` as the geometry reference for
the final composition. Guides are production aids and must not appear in the
export.

## Output contract

By default, deliver exactly two primary PNG files:

1. `<slug>-launcher-cover.png` — `1518 × 2076 px`;
2. `<slug>-<preset>.png` — exact dimensions of the selected print preset.

Also create `<slug>-artwork-manifest.json` beside them. The manifest is support
metadata, not a third image. Include:

```json
{
  "schema_version": 1,
  "game_title": "Example Game",
  "provider": "STEAM",
  "provider_game_id": "123",
  "media_id": "GAME-001",
  "preset": "floppy_35_label",
  "cover": { "width_px": 1518, "height_px": 2076 },
  "print_artwork": {
    "width_mm": 70,
    "height_mm": 52,
    "width_px": 827,
    "height_px": 614,
    "ppi": 300,
    "capacity_badge": "COMPRESSED CORE: 1.44 MB"
  },
  "sources": []
}
```

Record source URLs, retrieval dates, licenses or user authorization in
`sources`. Omit unknown optional IDs rather than inventing them.

When the user explicitly requests only one artifact for an iterative visual
test, generate only that artifact. Do not create the companion image or a new
manifest unless requested.

## Workflow

1. Normalize the request into the required inputs and select one preset.
2. Research imagery only when the user permits it or asks for it. Prefer
   official publisher/provider pages, user-owned files, or reusable licensed
   material. Record provenance immediately.
3. Analyze supplied images by role. A style reference is not automatically an
   edit target or permission to copy its logos and text.
4. Write one shared art brief for both outputs: focal subject, palette,
   lighting, title treatment, technical border, and print wear. Define the
   launcher cover's framed runtime-information panel separately from the
   printable artwork's provider treatment and lower metadata band.
5. Generate each final piece as one complete composition with the image-
   generation tool. The launcher cover contains game art, the game title, the
   MediaDeck border, and a deliberately designed lower runtime-information
   panel. The panel has its own palette-matched frame and a quiet inner field,
   but it must not bake provider or game metadata into that field. Compose the
   illustration and panel as one intentional layout from the beginning.
   Printable artwork contains its required provider treatment and metadata.
   Keep the two outputs visibly related, but recompose for each aspect ratio;
   do not merely stretch or crop a single finished layout.
6. Do not rebuild the generated design with Canvas, SVG overlays, scripted
   borders, or a separate code-compositing pass. If title, metadata, or border
   quality is wrong, correct it with an image edit or regenerate the complete
   piece while repeating the exact required text.
7. Export the exact pixel canvas defined in `references/dimensions.md`. When a
   generator ignores requested pixels, normalize with the smallest centered
   crop needed to match the target ratio, then use a high-quality resampler as
   documented in `references/export-normalization.md`. Never stretch one axis.
   Do not include crop marks in the primary PNG.
8. Inspect both files at original resolution. Check spelling, faces/hands,
   logos, edges, safe areas, and whether aging remains subtle.
9. Run:

   ```text
   python scripts/validate_artwork.py --cover <cover.png> --label <label.png> --preset <preset>
   ```

10. If validation or visual review fails, revise and re-run it. Never claim
    print readiness when the dimensions fail.

## Composition rules

- The launcher cover is a clean portrait collectible cover with less metadata.
- The launcher cover contains no baked-in metadata at all. Its only visible
  text is the exact game title in the upper title zone.
- Reserve the lower 12–14% of every launcher cover, preferably about 13%, for a purpose-built,
  palette-matched **runtime-information panel**. The panel must visibly belong
  to the cover: give it a complete angular or chamfered enclosure, a clear top
  separator, nested keylines, and restrained corner or side details derived
  from the main MediaDeck border. Its inner field must be dark and calm enough
  when the application later draws two compact lines of white and light-green
  text. The two-line requirement controls height only: the generated panel
  interior must be completely blank. The stated percentage includes the panel
  frame and its top separator; do not add extra empty padding. Keep faces,
  characters, logos,
  bright highlights, decorative copy, media IDs, provider names, and other
  important details out of the panel interior.
- Render absolutely no content inside the launcher panel: no text, IDs,
  provider names, placeholder rows, horizontal writing guides, field boxes,
  icons, badges, symbols, fake glyphs, pseudo-letters, or decorative microcopy.
  The MediaDeck application is the sole owner of everything displayed inside
  this panel at runtime.
- Never create the runtime-information panel by simply darkening, fading,
  blurring, masking, blacking out, erasing, or obscuring the lower part of an
  otherwise continuous illustration. The scene must be intentionally composed
  to end at the panel boundary, and the framed panel must be generated as a
  real visual component of the cover.
- The printable media artwork carries a lower technical strip with factual,
  legible metadata.
- Every cover and printable artwork uses the MediaDeck technical border defined
  in `references/art-direction.md`. Its geometry stays consistent, while its
  colors are derived from the game artwork rather than fixed to one palette.
- Use the exact game title once as the principal title. Avoid repeating it as
  decorative filler.
- On printable artwork, prefer a typographic provider badge: `PROVIDED BY
  STEAM`, `PROVIDED BY LOCAL`, or the supplied provider. Do not place a
  provider badge on the launcher cover. Use an official logo only when the
  source and its usage are authorized.
- A physical MediaDeck disc or floppy remains a key to a local installation.
  The final `CORE` line is a deliberate retro capacity badge for the selected
  medium, not the real size of the game or a claim that game files are stored
  there.
- Use the capacity badge fixed by the preset: `COMPRESSED CORE: 1.44 MB` for
  `floppy_35_label`, `OPTICAL CORE: 700 MB` for `cd_jewel_front`, and
  `OPTICAL CORE: 4.70 GB` for `dvd_case_front`.
- Never use `LOCAL INSTALLATION REQUIRED` on MediaDeck artwork.
- Appropriate printable-artwork footer fields are the MediaDeck line, media
  ID, provider, provider game ID, and the preset-specific `CORE` line. None of
  these fields belongs on the launcher cover.
- Never place executable paths, launch arguments, process names, secrets, or
  arbitrary text read from the physical medium on either image.
- Do not invent ratings, publisher legal text, certification marks, barcodes,
  edition names, platform logos, or a different capacity than the selected
  preset.

## Prompt structure

For each image-generation call, use labeled lines in this order:

```text
Use case: product-mockup / stylized-concept
Asset type: launcher cover or exact-size printable media artwork
Primary request: game and selected preset
Input images: identify every image by role
Subject: focal game imagery
Style/medium: premium retro-futurist printed game packaging
Composition/framing: aspect ratio, title zone, focal zone, and either the
framed launcher runtime-information panel or the printable metadata zone
Lighting/mood: game-appropriate and coherent across the pair
Text (verbatim): only the large text the generator must render
Materials/textures: subtle age, ink grain, edge wear
Constraints: exact hierarchy, no extra text/logos/watermark
Avoid: fake metadata, heavy damage, illegible type, stretched art
```

Generate the two primary images in separate calls. One call is not a substitute
for two aspect-ratio-specific compositions.

For a launcher cover, `Text (verbatim)` contains only the exact game title and
the prompt must explicitly require the palette-matched framed
runtime-information panel, forbid all text inside it, and forbid creating it by
fading, masking, darkening, or erasing the illustration.
For printable artwork, include every required visible string, including footer
metadata. The returned image must be the finished artwork, not a background or
an intermediate canvas awaiting later assembly.

## Quality gate

The pair is complete only when:

- both primary PNGs exist;
- dimensions match the selected profiles exactly;
- title and IDs are spelled correctly;
- the provider treatment is factual;
- the launcher cover has no baked-in metadata and its lower 12–14%, preferably
  about 13%, is a
  complete, intentional, palette-matched framed panel with a dark uncluttered
  and completely blank interior whose height can accommodate two compact lines
  added later by the application, without excess vertical padding;
- the launcher panel contains zero baked-in content, including placeholder
  lines, guides, field boxes, icons, fake glyphs, or decorative microcopy;
- the launcher artwork ends cleanly at the panel boundary instead of being
  hidden by a fade, mask, blur, blackout, vignette, or erased region;
- label text remains inside the safe area;
- no guide, crop mark, watermark, or accidental border remains;
- the intentional technical border is complete, color-matched to the game, and
  remains inside the trim/safe-area rules;
- the two pieces share palette, focal imagery, texture, and identity;
- the physical label is readable at 100% print size;
- provenance is present in the manifest;
- `validate_artwork.py` exits successfully.

If the customer names a specific label stock or supplies measurements, replace
the default physical preset with those exact dimensions and record the override
in the manifest. Manufacturer measurements take precedence over the defaults.

## Reference examples

Read `references/example-catalog.md` when choosing a border palette or adapting
one game identity across aspect ratios. The catalog points to validated Kakarot
and Need for Speed Underground files under `examples/`; they are visual
references, not templates to recolor mechanically.
