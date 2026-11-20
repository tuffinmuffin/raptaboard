# Fields

Field images and their metadata for the Field widget, in the format of WPILib's `fields` library:
one JSON per field, and the image it names (`field-image.path`, relative to the JSON).

- `name`, `season`, `game`: what the field is.
- `field-dimensions`: `length` (alliance wall to alliance wall) and `width`, in meters.
- `field-image`: the image, and where the field's edges are in it in pixels (`top`, `left`,
  `bottom`, `right`; x to the right, y down). The blue alliance wall is on the right.
- `field-tags`: the official AprilTag layout. Poses are in WPILib 2027's frame: origin at the
  center of the field on the floor, +X toward the blue alliance wall, +Y left facing +X.
- `raptaboard` (optional, raptaboard only): mapping details WPILib's format doesn't have.
  Reserved; nothing uses it yet.

raptaboard looks for fields in `fields/` beside where it's started, beside the program, and in
`~/.raptaboard/fields/`, and the 2026 REBUILT field is built into the program. To add a field,
put its JSON and image in one of those folders (in a subfolder is fine), or pick the JSON with
"Open field file…" in a Field widget's settings.

## Source and license

`frc/2026-rebuilt.png`, `frc/2026-rebuilt-welded.json` and `frc/2026-rebuilt-andymark.json` are from
WPILib, [allwpilib](https://github.com/wpilibsuite/allwpilib) at commit
`f25c4b317e99f4b278dbcf7214bf4ecb0b701c06`, `fields/src/main/native/resources/org/wpilib/fields/frc/`.
They're under WPILib's BSD license, copyright FIRST and other WPILib contributors; see
[`LICENSE-WPILib.md`](LICENSE-WPILib.md).
