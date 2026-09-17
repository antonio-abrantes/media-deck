# Age of Empires: Definitive Edition artwork test

## Deliverables

- launcher cover: `1518 × 2076 px`;
- 3.5-inch floppy label: `827 × 614 px`, 300 PPI;
- provider: `STEAM`;
- Steam AppID: `1017900`;
- MediaDeck media ID: `AOEDE-001`;
- capacity line: `COMPRESSED CORE: 1.44 MB`.

## Visual review

- Both pieces use newly generated, matched historical artwork.
- Each piece was generated as a complete final composition; no Canvas,
  scripted frame, or separately composited border was used.
- The title is present, prominent, and spelled correctly on both pieces.
- The borders share the MediaDeck angular technical language while allowing
  the image model to adapt their geometry naturally to each aspect ratio.
- Border colors use antique gold, deep teal, imperial crimson, aged bronze,
  and charcoal sampled from the artwork.
- The floppy metadata is complete, legible, and inside its lower information
  band.
- No photographed media body, case mockup, watermark, rating, barcode,
  executable path, process name, or launch argument appears.
- External pages were used only for factual reference; no source image was
  incorporated.

## Automated validation

Run `scripts/validate_artwork.py` with the launcher cover, floppy label,
`--preset floppy_35_label`, and `--strict-density`.

Expected result: both pixel dimensions pass and the printable label reports
300 PPI density metadata.
