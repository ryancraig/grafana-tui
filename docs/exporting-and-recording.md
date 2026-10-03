# Exporting and Recording

Grafatui can export the current dashboard view as SVG, PNG, or both. It can also record changed dashboard states into a timestamped frame bundle.

## Export a Snapshot

Press `e` to export the current visible dashboard.

Output files are written under `--export-dir`:

```bash
grafatui --export-dir ./grafatui-exports --export-format both
```

Supported formats:

- `svg`
- `png`
- `both`

## Narrow Panels and Long Legends

An export keeps each panel's text inside the panel. Stat values shrink to fit,
and labels too long for their space end in `…`. Axis labels that would collide
with the start and end times, or with the top and bottom values, are left out;
their grid lines stay.

A graph's legend takes the rows it needs, up to a third of the panel. Series
that do not fit are counted as `+N more`, in the terminal as well as in exports.
To see or export more of them, select the panel and press `f` for fullscreen,
then `e`: the export holds only that panel, and its legend gets a third of the
screen. Press `f` or `Esc` to return to the dashboard.

## Record Changed Frames

Press `Ctrl+E` to start recording. Press `Ctrl+E` again, or quit with `q`, to finalize the bundle.

Grafatui records only changed rendered states:

```text
grafatui-recording-<timestamp>/
  frame-000001.svg
  frame-000002.svg
  manifest.json
```

If `--export-format png` or `both` is selected, matching PNG files are written too.

When external annotations are visible, their panel targeting and applied tag
filter affect the markers written to SVG/PNG exports and changed-frame
recordings. Any active inline annotation details remain exportable. Annotation
modal chrome is omitted from exports and recordings, and draft-only tag-filter
edits do not create recording frames; a frame can change after the filter is
applied or cleared.

## Recording Limits

Limit the number of changed frames in one recording:

```bash
grafatui --record-max-frames 300
```

When the frame cap is reached, Grafatui stops writing new frames and records `completed_reason = "capped"` when finalized.

## Manifest

Each recording writes a `manifest.json` file with metadata for downstream tooling:

```json
{
  "version": 1,
  "format": "svg",
  "changed_only": true,
  "frame_count": 2,
  "max_frames": 300,
  "completed_reason": "stopped",
  "viewport": { "width": 100, "height": 40 },
  "frames": [
    {
      "index": 1,
      "elapsed_ms": 0,
      "files": ["frame-000001.svg"]
    }
  ]
}
```
