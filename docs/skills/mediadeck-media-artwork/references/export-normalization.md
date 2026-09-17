# Exact-size export normalization

Image generators commonly honor orientation but return a standard canvas. A
generated file is not print-ready until its final pixel dimensions match the
selected profile.

## Method

1. Inspect source width and height.
2. Compare source and target aspect ratios.
3. Center-crop only the surplus dimension. Do not stretch width or height
   independently.
4. Resize the cropped image with a high-quality Lanczos or equivalent filter.
5. Encode print artwork as PNG with 300 PPI density metadata.
6. Run `scripts/validate_artwork.py` with `--strict-density`.

Keep the original generated file outside the deliverable pair so a future edit
can return to the unnormalized source.

## FFmpeg reference commands

FFmpeg is one valid normalization tool when present. Replace crop and target
values with those calculated for the actual source.

```text
ffmpeg -i source.png -vf "crop=<w>:<h>:<x>:<y>,scale=1518:2076:flags=lanczos" -frames:v 1 -c:v png -pred mixed -compression_level 9 cover.png
```

```text
ffmpeg -i source.png -vf "crop=<w>:<h>:<x>:<y>,scale=827:614:flags=lanczos" -frames:v 1 -c:v png -pred mixed -compression_level 9 -dpi 300 floppy-label.png
```

Do not copy these crop values blindly: calculate them from the actual source.
The `-dpi 300` PNG option writes density metadata supported by the validator.

## Visual reinspection

After normalization, inspect the final PNG again. Reject the result if the crop
cuts a title, face, provider badge, media ID, frame, or safe-area content. In
that case, regenerate with more negative space instead of forcing the crop.
