# MediaDeck artwork direction

## Shared identity

The launcher cover and physical-media artwork form one collectible set. Reuse:

- the same focal character or scene;
- the same dominant palette and highlight color;
- one title treatment;
- one provider badge treatment on printable artwork only;
- restrained print grain and edge wear;
- the same media ID when supplied.

Recompose instead of stretching. The portrait launcher cover, landscape floppy
label, square jewel insert, and tall DVD front each need their own crop and
hierarchy.

## Mandatory MediaDeck technical border

Every launcher cover, floppy label, CD jewel front, and DVD front uses the same
border language. This is a MediaDeck identity element, not an optional flourish:

- angular outer silhouette or clipped/chamfered corner treatment;
- two or three nested keylines surrounding the image;
- short circuit-like traces, right-angle paths, nodes, small blocks, and sparse
  technical marks integrated into the frame;
- a dark inner separator between the frame and the principal artwork;
- subtle printed wear shared with the rest of the image;
- flat front-facing geometry with no case, disc, floppy body, perspective, or
  product mockup.

The geometry remains recognizable across games, but the colors must be sampled
from the specific artwork. Choose one dominant frame color, one highlight, and
one small accent from the game palette. Do not default every game to the red,
yellow, and cyan palette used by the Kakarot example.

Examples:

- fiery anime art may use red/orange with cyan accents;
- night street-racing art may use asphalt black, metallic silver, neon green,
  electric blue, or violet sampled from the scene;
- a colder game may use steel blue, ice cyan, and a restrained warm accent.

Background color and texture extend to the trim edge. Keep the principal inner
keyline, title, nodes, and metadata inside the safe inset. The border must remain
complete after exact-size normalization; if cropping damages it, regenerate
with more margin instead of accepting a broken frame.

## Launcher cover

- Treat it as premium box art, not a technical form.
- Keep the upper title zone readable at the runtime display size.
- Let the illustrated section occupy the upper 86–88% of the canvas. Keep the
  focal subject prominent through the middle of that section and end important
  artwork above the runtime-information panel.
- Reserve the lower 12–14%, with a preferred target of about 13%, as a
  **framed runtime-information panel**. The percentage is the panel's complete
  exterior height, including its top separator and bottom frame. At the source
  size this is approximately the bottom `249–291 px`, preferably about
  `270 px`; at the `253 × 346 px` runtime aperture it is approximately the
  bottom `42–48 px`, preferably about `45 px`.
- Treat the panel as a real piece of the cover design, not empty space. It must
  have a visible top boundary and a complete enclosure: angular or chamfered
  corners, a dark structural rim, two nested palette-matched keylines, and a
  few restrained side or corner accents related to the outer technical frame.
  The outer frame and the panel frame should appear mechanically joined.
- Use a near-black, charcoal, deep navy, dark crimson, or similarly dark inner
  fill derived from the cover palette. A subtle material texture or gentle
  tonal variation is allowed inside the panel, but the field must remain calm
  and provide strong contrast when the application later draws two compact
  lines of white and light-green runtime labels. The two-line count determines
  panel height only. Keep the frame compact and do not create a tall empty
  header, oversized margins, or unused padding inside the panel.
- Keep the panel interior free of baked text, media ID, provider, game ID,
  badges, logos, faces, characters, bright effects, scenery silhouettes,
  strong nodes, and high-contrast ornament. The application owns all text
  displayed inside it.
- The generated panel interior must be completely blank. Do not render text,
  IDs, provider names, placeholder rows, writing guides, field boxes, icons,
  badges, symbols, fake glyphs, pseudo-letters, or decorative microcopy. Any such
  content is a failed cover and must be regenerated, not accepted.
- Compose the illustration to terminate deliberately at the panel's top frame.
  **Never** obtain the panel by laying a dark gradient, vignette, blur, mask,
  black rectangle, shadow, fog, or other occlusion over the lower portion of a
  continuous image. Never make the character or scenery look erased, covered,
  faded out, or cut off by darkness.
- Continue the outer technical border around the perimeter and visually connect
  it to the panel enclosure. Keep decorative marks near the panel edges so they
  support the frame without occupying the label field.
- The only text rendered into a launcher cover is the exact game title in the
  upper title zone. Printable artwork retains the factual metadata described
  below.

## Floppy label

The visual references supplied with the MediaDeck project establish this
language:

- bold game art in the upper 60–68%;
- high-contrast title integrated with the image;
- lower dark information band occupying 28–35%;
- colored frame or circuit-like accent lines around the art;
- compact condensed/monospaced metadata;
- lightly aged ink, tiny scratches and paper wear;
- no photographed floppy body in the exported label itself.

The label must remain a flat, front-facing print file. Do not generate a product
mockup, hand, desk, perspective, adhesive backing, or surrounding white page.

## CD jewel front

- Square front insert only.
- Use the extra width for environmental art or supporting silhouettes.
- Keep title and provider inside the 3 mm safe inset.
- A small lower technical strip may echo the floppy label, but should not turn
  the square cover into a form.

## DVD case front

- Tall front panel only; no spine and no back cover.
- Use a cinematic vertical composition with title in the upper quarter and
  metadata in the lower fifth.
- Do not add a plastic case, disc, mockup perspective, barcode, rating badge, or
  legal block unless the user provides authorized assets and exact copy.

## Aging and material

Aim for a collectible printed object that has been handled, not damaged:

- subtle halftone or ink grain;
- slight desaturation variation;
- restrained edge rub and hairline scratches;
- mild paper/fiber texture;
- crisp title and metadata despite the aging.

Avoid torn corners, stains over faces or text, heavy grunge, strong blur,
low-resolution pixelation, fake folds, and random illegible microcopy.

## Factual metadata hierarchy

Use only supplied facts. Recommended order:

```text
MEDIADECK PHYSICAL GAME KEY
MEDIA ID: GAME-001
PROVIDER: STEAM
STEAM APP: 851850
COMPRESSED CORE: 1.44 MB
```

The final line changes with the physical preset:

| Preset | Required final line |
| --- | --- |
| `floppy_35_label` | `COMPRESSED CORE: 1.44 MB` |
| `cd_jewel_front` | `OPTICAL CORE: 700 MB` |
| `dvd_case_front` | `OPTICAL CORE: 4.70 GB` |

These are retro capacity badges for the media class, not measurements of the
installed game or claims that game files are stored on the physical medium.
Never use `LOCAL INSTALLATION REQUIRED` on a MediaDeck label or insert. Do not
invent installer names, software versions, other capacities, or fake legal
copy.

## Typography

- Principal title: faithful supplied game wordmark when authorized, otherwise
  a strong custom display treatment.
- Metadata: condensed sans or technical monospaced family.
- Provider: text badge by default on printable artwork; never bake it into the
  launcher cover.
- Render exact copy once; no decorative repeated words.
- If the generator produces incorrect text, use an image-edit iteration or
  regenerate the complete composition with the exact strings repeated. Do not
  rebuild the piece with Canvas or scripted layout as the default correction.
