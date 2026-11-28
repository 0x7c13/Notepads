# Script for generating glyph outline data in both raw (points, contours, tag) and
# path (move_to, line_to, etc) commands for all glyphs in a font at various sizes.

import sys
import os
import freetype

# Our requirements.txt pins freetype-py to version 2.4.0 which includes FreeType 2.13.0. We only
# want to track one FreeType version at a time, so ensure that we are consistent.
assert freetype.version() == (2, 13, 0)

# Each glyph will be sampled in these sizes (in pixels per em). A size of 0 indicates
# an unscaled glyph (results in font units)
SAMPLE_SIZES = [0, 16, 50]

# For variable fonts, sample the glyphs at these normalized coordinates.
# The odd intermediate numbers are chosen so that rounding behavior yields consistent
# results among FreeType, freetype-py and read-fonts.
SAMPLE_COORDS = [-1.0, -0.2000122, 0.0, 0.2999878, 1.0]


class DecomposeContext:
    def __init__(self, is_scaled: bool, is_cff: bool):
        self.data = ""
        self.is_scaled = is_scaled
        self.is_cff = is_cff
        self.last_end = None
        self.last_move = None

    def add_element(self, cmd, points):
        SCALE_FACTOR = 1.0 / 64.0
        self.data += cmd + " "
        if self.is_scaled:
            for point in points:
                self.data += " {},{}".format(point.x *
                                             SCALE_FACTOR, point.y * SCALE_FACTOR)
        else:
            for point in points:
                self.data += " {},{}".format(point.x, point.y)
        self.last_end = points[-1]
        self.data += "\n"


def path_move_to(pt, ctx):
    ctx.add_element("m", [pt])


def path_line_to(pt, ctx):
    # FreeType removes some (but not all!) degenerate lines for CFF outlines...
    # Remove the rest here for consistency.
    if not ctx.is_cff or ctx.last_end != pt:
        ctx.add_element("l", [pt])


def path_quad_to(c, pt, ctx):
    ctx.add_element("q", [c, pt])


def path_cubic_to(c1, c2, pt, ctx):
    ctx.add_element("c", [c1, c2, pt])


class GlyphData:
    def __init__(self):
        self.data = ""

    def add_glyph(self, face: freetype.Face, size, glyph_id, coords=[], hinting="none"):
        face.set_pixel_sizes(size, size)
        flags = freetype.FT_LOAD_NO_AUTOHINT | freetype.FT_LOAD_NO_BITMAP
        if hinting == "full":
            flags |= freetype.FT_LOAD_TARGET_NORMAL
        elif hinting == "light":
            flags |= freetype.FT_LOAD_TARGET_LIGHT
        elif hinting == "light-subpixel":
            flags |= freetype.FT_LOAD_TARGET_LCD
        else:
            flags |= freetype.FT_LOAD_NO_HINTING
            hinting = "none"
        if size == 0:
            flags |= freetype.FT_LOAD_NO_SCALE
            # freetype doesn't like pixel sizes of 0
            face.set_pixel_sizes(16, 16)
        if len(coords):
            face.set_var_blend_coords(coords)
        face.load_glyph(glyph_id, flags)
        self.data += "glyph {} {} {}\n".format(glyph_id, size, hinting)
        if len(coords) != 0:
            self.data += "coords"
            for coord in coords:
                self.data += " " + str(coord)
            self.data += "\n"
        self.data += "contours"
        for contour in face.glyph.outline.contours:
            self.data += " " + str(contour)
        self.data += "\npoints"
        for point in face.glyph.outline.points:
            self.data += " {},{}".format(point[0], point[1])
        self.data += "\ntags"
        for tag in face.glyph.outline.tags:
            self.data += " " + str(tag)
        self.data += "\n"
        decompose_ctx = DecomposeContext(size != 0, face.get_format() == 'CFF')
        face.glyph.outline.decompose(
            context=decompose_ctx, move_to=path_move_to, line_to=path_line_to, conic_to=path_quad_to, cubic_to=path_cubic_to)
        self.data += decompose_ctx.data
        self.data += "-\n"


font_path = sys.argv[1]

font_dir = os.path.abspath(os.path.dirname(os.path.dirname(font_path)))
out_dir = os.path.join(font_dir, "extracted")
out_path = os.path.join(out_dir, os.path.splitext(
    os.path.basename(font_path))[0]) + "-glyphs.txt"

try:
    face = freetype.Face(font_path)
    # make sure we have scalable outlines
    assert(face.is_scalable)
except:
    # some of our fonts are not complete (e.g. missing hhea table) and will fail to
    # load in FreeType
    print("Skipping outline extraction for \"%s\"" % font_path)
    exit(0)

print("Extracting glyphs from \"%s\" to \"%s\"..." % (font_path, out_path))

axis_count = 0

try:
    axis_count = len(face.get_var_design_coords())
except:
    pass

glyphs = GlyphData()

if axis_count > 0:
    for coord in SAMPLE_COORDS:
        coords = [coord] * axis_count
        for glyph_id in range(0, face.num_glyphs):
            for size in SAMPLE_SIZES:
                glyphs.add_glyph(face, size, glyph_id, coords)
else:
    for glyph_id in range(0, face.num_glyphs):
        for size in SAMPLE_SIZES:
            glyphs.add_glyph(face, size, glyph_id)

f = open(out_path, "w")
f.write(glyphs.data)
f.close()
