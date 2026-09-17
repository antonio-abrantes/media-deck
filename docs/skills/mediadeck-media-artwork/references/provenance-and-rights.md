# Image provenance and rights

## Source priority

Use sources in this order:

1. images supplied by the user with explicit permission to reuse;
2. official publisher or provider artwork whose intended use is compatible;
3. licensed or public-domain material with recorded terms;
4. newly generated artwork guided by factual game/theme descriptions;
5. web images used only as research/style references, not copied into output.

## Internet research

When internet research is requested:

- prefer the official game/publisher/provider page;
- record direct source URL, page title, retrieval date, and license or stated
  authorization in the manifest;
- distinguish inspiration/reference from a file actually incorporated;
- do not remove watermarks or crop away ownership marks;
- do not infer that search-result availability grants commercial reuse;
- do not download or redistribute a source when its permission is unclear.

## Supplied references

Label every supplied image by role before generation:

```text
Image 1: style reference only
Image 2: authorized game artwork to incorporate
Image 3: provider logo with customer-provided usage permission
```

A style reference controls palette, texture, hierarchy, or mood. It does not
authorize copying unrelated trademarks, text, logos, characters, or layouts
pixel-for-pixel.

## Provider branding

Default to a text badge such as `PROVIDED BY STEAM`. Use a provider logo only
when an approved local asset or explicit permission exists. Never synthesize a
near-copy of a logo from memory.

## Manifest source entry

```json
{
  "url": "https://example.invalid/official-art",
  "title": "Official key art",
  "retrieved_at": "2026-09-16",
  "role": "visual_reference",
  "license_or_permission": "reference only; not embedded"
}
```

If provenance cannot be established, generate original supporting imagery and
state that external material was used only for research.
