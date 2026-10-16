# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

"""Generate original, deterministic legacy color-font fixtures with fonttools 4.60.1.

These are geometric test drawings, not derived from installed or third-party
fonts. Only regeneration uses Python/fonttools; native tests load the checked-in
TTF files privately through DirectWrite and never install fonts.
"""

from pathlib import Path
import struct
import zlib

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import newTable
from fontTools.ttLib.tables.sbixGlyph import Glyph
from fontTools.ttLib.tables.sbixStrike import Strike


def rectangle(left, bottom, right, top):
    pen = TTGlyphPen(None)
    if left != right:
        pen.moveTo((left, bottom))
        pen.lineTo((left, top))
        pen.lineTo((right, top))
        pen.lineTo((right, bottom))
        pen.closePath()
    return pen.glyph()


def base(kind):
    family = f"Notepads Test {kind}"
    builder = FontBuilder(1000, isTTF=True)
    glyphs = {
        ".notdef": rectangle(50, 0, 850, 750),
        "space": rectangle(0, 0, 0, 0),
        "color": rectangle(50, 0, 950, 750),
        "foreground": rectangle(50, 0, 450, 750),
        "accent": rectangle(450, 0, 950, 750),
        "translucent": rectangle(250, 200, 750, 550),
    }
    builder.setupGlyphOrder(list(glyphs))
    builder.setupCharacterMap({0x20: "space", 0xE000: "color"})
    builder.setupGlyf(glyphs)
    bearings = {".notdef": 50, "space": 0, "color": 50, "foreground": 50, "accent": 450, "translucent": 250}
    builder.setupHorizontalMetrics({name: (1000 if name != "space" else 350, bearings[name]) for name in glyphs})
    builder.setupHorizontalHeader(ascent=800, descent=-200, lineGap=0)
    builder.setupNameTable({
        "familyName": family,
        "styleName": "Regular",
        "uniqueFontIdentifier": family + " 1.0",
        "fullName": family,
        "psName": family.replace(" ", ""),
        "version": "Version 1.000",
        "copyright": "Copyright (c) 2026 Jiaqi (0x7c13) Liu",
        "licenseDescription": "MIT License; see repository LICENSE.txt. Original geometric test asset.",
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, sTypoLineGap=0,
                     usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    builder.font.recalcTimestamp = False
    builder.font["head"].created = builder.font["head"].modified = 2082844800
    return builder


def png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = bytearray()
    for y in range(32):
        rows.append(0)
        for x in range(32):
            color = (240, 20, 40, 255) if x < 16 else (20, 180, 60, 255)
            if 8 <= x < 24 and 8 <= y < 24:
                color = (20, 40, 240, 128)
            rows.extend(color)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 32, 32, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def main():
    destination = Path(__file__).with_name("Fonts")
    destination.mkdir(exist_ok=True)
    colr = base("COLRv0")
    colr.setupCOLR({"color": [("foreground", 0xFFFF), ("accent", 0), ("translucent", 1)]}, version=0)
    colr.setupCPAL([[(0.9, 0.05, 0.1, 1.0), (0.05, 0.1, 0.95, 0.5)]])
    colr.save(destination / "LegacyColr.ttf")

    svg = base("SVG")
    table = svg.font["SVG "] = newTable("SVG ")
    table.docList = [(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -800 1000 1000">'
        '<g id="glyph2"><rect x="50" y="-750" width="400" height="750" fill="context-fill"/>'
        '<rect x="450" y="-750" width="500" height="750" fill="#e60d1a"/>'
        '<rect x="250" y="-550" width="500" height="350" fill="#0d1af2" fill-opacity="0.5"/>'
        '</g></svg>', 2, 2)]
    svg.save(destination / "LegacySvg.ttf")

    bitmap = base("Bitmap")
    table = bitmap.font["sbix"] = newTable("sbix")
    strike = Strike(ppem=32, resolution=96)
    strike.glyphs["color"] = Glyph(glyphName="color", graphicType="png ",
                                   originOffsetX=0, originOffsetY=-6, imageData=png())
    table.strikes[32] = strike
    bitmap.save(destination / "LegacyBitmap.ttf")
    for path in sorted(destination.glob("*.ttf")):
        print(f"{path.name}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
