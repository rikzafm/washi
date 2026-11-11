#!/usr/bin/env python3
"""Font tooling for ticket 03 (font matching).

  python tools/fonts.py fetch  [dir]        download the OFL candidate fonts (default target/font-raw)
  python tools/fonts.py subset <src> <dst>  subset every .ttf in <src> into <dst> (Latin + Latin-1 + punctuation)
  python tools/fonts.py embolden <src> <dst> <ratio> <family>        real bold face from outlines
  python tools/fonts.py slant <src> <dst> <skew> <family> plain|bold  real italic face by shearing
  python tools/fonts.py blank <src> <dst> <family>               same advances, no outlines (edit fields)

`subset` needs fonttools:  pip install fonttools
"""
import os
import sys
import urllib.request

BASE = "https://raw.githubusercontent.com/google/fonts/main/ofl/"

# Path under google/fonts/ofl, saved as <file>. OFL.txt is saved as OFL-<family dir>.txt.
CANDIDATES = """
shadowsintolight/ShadowsIntoLight.ttf
shadowsintolighttwo/ShadowsIntoLightTwo-Regular.ttf
reeniebeanie/ReenieBeanie.ttf
indieflower/IndieFlower-Regular.ttf
gochihand/GochiHand-Regular.ttf
kalam/Kalam-Bold.ttf
kalam/Kalam-Regular.ttf
patrickhand/PatrickHand-Regular.ttf
caveatbrush/CaveatBrush-Regular.ttf
tinos/Tinos-Regular.ttf
tinos/Tinos-Bold.ttf
tinos/Tinos-Italic.ttf
crimsontext/CrimsonText-Regular.ttf
crimsontext/CrimsonText-Bold.ttf
crimsontext/CrimsonText-Italic.ttf
ptserif/PT_Serif-Web-Regular.ttf
ptserif/PT_Serif-Web-Bold.ttf
ptserif/PT_Serif-Web-Italic.ttf
cardo/Cardo-Regular.ttf
cardo/Cardo-Bold.ttf
cardo/Cardo-Italic.ttf
gentiumbookplus/GentiumBookPlus-Regular.ttf
gentiumbookplus/GentiumBookPlus-Bold.ttf
gentiumbookplus/GentiumBookPlus-Italic.ttf
oldstandardtt/OldStandard-Regular.ttf
oldstandardtt/OldStandard-Bold.ttf
oldstandardtt/OldStandard-Italic.ttf
""".split()

# Latin, Latin-1, general punctuation (dashes, quotes, bullet, ellipsis), euro, trademark, minus.
UNICODES = "U+0020-007E,U+00A0-00FF,U+2010-2027,U+2030-203A,U+20AC,U+2122,U+2212"


# Variable fonts: (dir, file in the repo, output stem, axis values). Cut into static faces, because
# the renderer picks a candidate by file.
VARIABLE = [
    ("stixtwotext", "STIXTwoText[wght].ttf", "STIXTwoText-Regular", {"wght": 400}),
    ("stixtwotext", "STIXTwoText[wght].ttf", "STIXTwoText-Bold", {"wght": 700}),
    ("stixtwotext", "STIXTwoText-Italic[wght].ttf", "STIXTwoText-Italic", {"wght": 400}),
    ("notoserif", "NotoSerif[wdth,wght].ttf", "NotoSerif-Regular", {"wght": 400, "wdth": 100}),
    ("notoserif", "NotoSerif[wdth,wght].ttf", "NotoSerif-Bold", {"wght": 700, "wdth": 100}),
    ("notoserif", "NotoSerif-Italic[wdth,wght].ttf", "NotoSerif-Italic", {"wght": 400, "wdth": 100}),
    ("gelasio", "Gelasio[wght].ttf", "Gelasio-Regular", {"wght": 400}),
    ("gelasio", "Gelasio[wght].ttf", "Gelasio-Bold", {"wght": 700}),
    ("gelasio", "Gelasio-Italic[wght].ttf", "Gelasio-Italic", {"wght": 400}),
]


def fetch_variable(dest):
    import urllib.parse
    from fontTools.ttLib import TTFont
    from fontTools.varLib import instancer

    for family, name, stem, axes in VARIABLE:
        data = urllib.request.urlopen(BASE + family + "/" + urllib.parse.quote(name), timeout=60).read()
        tmp = os.path.join(dest, "_var.ttf")
        open(tmp, "wb").write(data)
        font = instancer.instantiateVariableFont(TTFont(tmp), axes)
        font.save(os.path.join(dest, stem + ".ttf"))
        os.remove(tmp)
        print("ok", stem)
        try:
            lic = urllib.request.urlopen(BASE + family + "/OFL.txt", timeout=60).read()
            open(os.path.join(dest, f"OFL-{family}.txt"), "wb").write(lic)
        except Exception as e:
            print("no OFL.txt for", family, e)


def fetch(dest):
    os.makedirs(dest, exist_ok=True)
    fetch_variable(dest)
    families = set()
    for path in CANDIDATES:
        family, name = path.split("/")
        families.add(family)
        data = urllib.request.urlopen(BASE + path, timeout=60).read()
        open(os.path.join(dest, name), "wb").write(data)
        print("ok", path)
    for family in sorted(families):
        try:
            data = urllib.request.urlopen(BASE + family + "/OFL.txt", timeout=60).read()
        except Exception as e:  # some families ship no OFL.txt in the repo
            print("no OFL.txt for", family, e)
            continue
        open(os.path.join(dest, f"OFL-{family}.txt"), "wb").write(data)


def subset(src, dst):
    from fontTools import subset as ft_subset
    from fontTools.ttLib import TTFont

    os.makedirs(dst, exist_ok=True)
    for name in sorted(os.listdir(src)):
        if not name.lower().endswith(".ttf"):
            continue
        opts = ft_subset.Options()
        opts.layout_features = ["*"]
        opts.name_IDs = ["*"]
        opts.notdef_outline = True
        opts.glyph_names = False
        opts.hinting = False
        font = TTFont(os.path.join(src, name))
        sub = ft_subset.Subsetter(opts)
        sub.populate(unicodes=ft_subset.parse_unicodes(UNICODES))
        sub.subset(font)
        out = os.path.join(dst, name)
        font.save(out)
        print(f"{name}: {os.path.getsize(os.path.join(src, name))} -> {os.path.getsize(out)} bytes")


def embolden(src, dst, ratio, family):
    """Make a real bold face by growing every outline by `ratio` x em (total stroke width), the way
    Skia's synthetic bold does (ratio 1/32 at large sizes). Advances are kept, as Skia keeps them.
    Needs fonttools and skia-pathops (pip install fonttools skia-pathops)."""
    import pathops
    from fontTools.pens.cu2quPen import Cu2QuPen
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib import TTFont

    font = TTFont(src)
    upm = font["head"].unitsPerEm
    width = ratio * upm
    gs = font.getGlyphSet()
    glyf = font["glyf"]
    for name in font.getGlyphOrder():
        path = pathops.Path()
        gs[name].draw(path.getPen(glyphSet=gs))
        if not len(path):
            continue
        grown = pathops.Path(path)
        grown.stroke(width, pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4)
        grown.convertConicsToQuads()
        merged = pathops.op(path, grown, pathops.PathOp.UNION)
        merged.convertConicsToQuads()
        pen = TTGlyphPen(None)
        merged.draw(Cu2QuPen(pen, max_err=1.0, reverse_direction=True))
        glyf[name] = pen.glyph()
    # a bold face: names, weight class, macStyle
    os2 = font["OS/2"]
    os2.usWeightClass = 700
    os2.fsSelection = (os2.fsSelection & ~0x40) | 0x20
    font["head"].macStyle |= 1
    for rec in font["name"].names:
        if rec.nameID in (1, 16):
            rec.string = family
        elif rec.nameID in (2, 17):
            rec.string = "Bold"
        elif rec.nameID == 4:
            rec.string = family + " Bold"
        elif rec.nameID == 6:
            rec.string = family.replace(" ", "") + "-Bold"
        elif rec.nameID == 3:
            rec.string = family.replace(" ", "") + "-Bold;emboldened"
    font.save(dst)
    print("wrote", dst, os.path.getsize(dst), "bytes")


def slant(src, dst, skew, family, bold):
    """Make a real italic face by shearing every outline (x += skew * y), the way Skia's synthetic
    italic does (skew 0.25). Needs fonttools."""
    import math
    from fontTools.pens.transformPen import TransformPen
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib import TTFont

    font = TTFont(src)
    gs = font.getGlyphSet()
    glyf = font["glyf"]
    for name in font.getGlyphOrder():
        pen = TTGlyphPen(gs)
        gs[name].draw(TransformPen(pen, (1, 0, skew, 1, 0, 0)))
        glyf[name] = pen.glyph()
    os2 = font["OS/2"]
    os2.fsSelection = (os2.fsSelection & ~0x40) | 0x01
    font["head"].macStyle |= 2
    font["post"].italicAngle = -math.degrees(math.atan(skew))
    style = "Bold Italic" if bold else "Italic"
    for rec in font["name"].names:
        if rec.nameID in (1, 16):
            rec.string = family
        elif rec.nameID in (2, 17):
            rec.string = style
        elif rec.nameID == 4:
            rec.string = f"{family} {style}"
        elif rec.nameID == 6:
            rec.string = family.replace(" ", "") + "-" + style.replace(" ", "")
        elif rec.nameID == 3:
            rec.string = family.replace(" ", "") + "-" + style.replace(" ", "") + ";slanted"
    font.save(dst)
    print("wrote", dst, os.path.getsize(dst), "bytes")


# Ascent and descent of the blank face, in font units (of 1024). A field's caret and selection are
# as tall as the larger of the line pitch and ascent + descent, so the sum stays under every line
# pitch of the note (the lowest is 1.22 em) and the caret is the line box on every line.
# `caret-line-em` in washi/ui/theme.slint is (ascent + descent) / units per em: keep them together.
BLANK_ASCENT, BLANK_DESCENT = 861, 100


def blank(src, dst, family):
    """A face with the advances of `src` and no ink: the edit fields lay their text out (caret,
    selection, line breaks) in it while the app draws the styled words itself. Its ascent and
    descent are cut down (see BLANK_ASCENT). Needs fonttools."""
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib import TTFont

    font = TTFont(src)
    glyf = font["glyf"]
    for name in font.getGlyphOrder():
        glyf[name] = TTGlyphPen(None).glyph()
    hhea, os2 = font["hhea"], font["OS/2"]
    hhea.ascent, hhea.descent, hhea.lineGap = BLANK_ASCENT, -BLANK_DESCENT, 0
    os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = BLANK_ASCENT, -BLANK_DESCENT, 0
    os2.usWinAscent, os2.usWinDescent = BLANK_ASCENT, BLANK_DESCENT
    for rec in font["name"].names:
        if rec.nameID in (1, 16):
            rec.string = family
        elif rec.nameID in (2, 17):
            rec.string = "Regular"
        elif rec.nameID == 4:
            rec.string = family
        elif rec.nameID == 6:
            rec.string = family.replace(" ", "") + "-Regular"
        elif rec.nameID == 3:
            rec.string = family.replace(" ", "") + "-Regular;blank"
    font.save(dst)
    print("wrote", dst, os.path.getsize(dst), "bytes")


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "blank":
        blank(sys.argv[2], sys.argv[3], sys.argv[4])
        sys.exit(0)
    if len(sys.argv) == 6 and sys.argv[1] == "embolden":
        embolden(sys.argv[2], sys.argv[3], float(sys.argv[4]), sys.argv[5])
    elif len(sys.argv) == 7 and sys.argv[1] == "slant":
        slant(sys.argv[2], sys.argv[3], float(sys.argv[4]), sys.argv[5], sys.argv[6] == "bold")
    elif len(sys.argv) >= 2 and sys.argv[1] == "fetch":
        fetch(sys.argv[2] if len(sys.argv) > 2 else "target/font-raw")
    elif len(sys.argv) == 4 and sys.argv[1] == "subset":
        subset(sys.argv[2], sys.argv[3])
    else:
        print(__doc__)
        sys.exit(2)
