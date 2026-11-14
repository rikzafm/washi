#!/usr/bin/env python3
"""Generates the tiny synthetic fixture fonts used by the font engine tests (needs fonttools).

  python tools/make_test_fonts.py [out_dir]     default: tools/snap/tests/fixtures/fonts

Everything here is drawn from scratch (boxes with a hole), so the fixtures carry no third-party
outlines; they are released under the SIL Open Font License 1.1 (OFL.txt in the zips).

  TestSans-VariableFont_wdth,wght.ttf   variable, wght 300..800 (default 400), wdth 75..100 (default 100)
  TestSlant-VariableFont_slnt.ttf       variable, only slnt -10..0 (leans right when negative)
  TestSerif-Regular.ttf / -Bold.ttf     single style, family "Test Serif"
  Test_Sans.zip                         laid out like a Google Fonts download of a variable family
  Test_Serif.zip                        static only (Regular, Bold, Italic in static/)
  Test_Plain.zip                        one Regular .ttf and nothing else
"""
import io
import os
import sys
import zipfile

from fontTools.designspaceLib import AxisDescriptor, DesignSpaceDocument, SourceDescriptor
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.varLib import build as varlib_build

UPEM = 1000
CHARS = (
    [" "]
    + [chr(c) for c in range(0x30, 0x3A)]
    + [chr(c) for c in range(0x41, 0x5B)]
    + [chr(c) for c in range(0x61, 0x7B)]
)
OFL = """Copyright (c) 2026 The Washi authors. These test fonts are original synthetic shapes.

This Font Software is licensed under the SIL Open Font License, Version 1.1.
The license is available with a FAQ at: https://openfontlicense.org
"""


def gname(ch):
    return "space" if ch == " " else ("uni%04X" % ord(ch))


def glyph(ch, stem, width_scale, advance_bias, shear=0.0):
    """A box with a hole. Same contour structure at every master (needed for interpolation)."""
    adv = round((520 + (ord(ch) % 5) * 40 + advance_bias) * width_scale)
    pen = TTGlyphPen(None)
    if ch == " ":
        return pen.glyph(), adv
    h = 520 if ch.islower() else 700
    x0, x1 = round(60 * width_scale), adv - round(60 * width_scale)
    s = round(stem * width_scale)
    low = -180 if ch in "gjpqy" else 0
    def p(x, y):
        return (round(x + shear * y), y)

    pen.moveTo(p(x0, low)); pen.lineTo(p(x0, h)); pen.lineTo(p(x1, h)); pen.lineTo(p(x1, low)); pen.closePath()
    pen.moveTo(p(x0 + s, low + s)); pen.lineTo(p(x1 - s, low + s)); pen.lineTo(p(x1 - s, h - s)); pen.lineTo(p(x0 + s, h - s)); pen.closePath()
    return pen.glyph(), adv


def build(family, style, stem, width_scale, bias, weight_class, italic=False, bold=False, shear=0.0):
    order = [".notdef"] + [gname(c) for c in CHARS]
    fb = FontBuilder(UPEM, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({ord(c): gname(c) for c in CHARS})
    glyphs, metrics = {}, {}
    g, a = glyph("H", stem, width_scale, bias, shear)
    glyphs[".notdef"], metrics[".notdef"] = g, (a, round(60 * width_scale))
    for c in CHARS:
        g, a = glyph(c, stem, width_scale, bias, shear)
        glyphs[gname(c)] = g
        metrics[gname(c)] = (a, 0 if c == " " else round(60 * width_scale))
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=900, descent=-300)
    ps = (family + "-" + style).replace(" ", "")
    fb.setupNameTable({"familyName": family, "styleName": style, "uniqueFontIdentifier": ps,
                       "fullName": family + " " + style, "psName": ps, "version": "Version 1.000"})
    fb.setupOS2(usWeightClass=weight_class, sTypoAscender=900, sTypoDescender=-300, usWinAscent=900,
                usWinDescent=300, fsSelection=(0x20 if bold else 0x40) | (0x01 if italic else 0))
    fb.setupPost()
    fb.setupHead(unitsPerEm=UPEM)
    return fb.font


def save(font):
    b = io.BytesIO()
    font.save(b)
    return b.getvalue()


def variable(workdir):
    d = DesignSpaceDocument()
    for tag, name, lo, df, hi in (("wght", "Weight", 300, 400, 800), ("wdth", "Width", 75, 100, 100)):
        a = AxisDescriptor()
        a.tag, a.name, a.minimum, a.default, a.maximum = tag, name, lo, df, hi
        d.addAxis(a)
    tmp = []
    for name, w, wd in (("def", 400, 100), ("light", 300, 100), ("heavy", 800, 100), ("narrow", 400, 75)):
        f = build("Test Sans", "Regular", 40 + (w - 300) * 0.35, wd / 100.0, (w - 400) * 0.12, 400)
        p = os.path.join(workdir, "_master_%s.ttf" % name)
        f.save(p)
        tmp.append(p)
        s = SourceDescriptor()
        s.path, s.name, s.location = p, name, {"Weight": w, "Width": wd}
        d.addSource(s)
    vf, _, _ = varlib_build(d)
    data = save(vf)
    for p in tmp:
        os.remove(p)
    return data


def slanted(workdir):
    """A variable font with only a `slnt` axis (-10..0): leaning right is negative, as in OpenType."""
    d = DesignSpaceDocument()
    a = AxisDescriptor()
    a.tag, a.name, a.minimum, a.default, a.maximum = "slnt", "Slant", -10, 0, 0
    d.addAxis(a)
    tmp = []
    for name, v, shear in (("up", 0, 0.0), ("lean", -10, 0.17633)):
        p = os.path.join(workdir, "_master_%s.ttf" % name)
        build("Test Slant", "Regular", 55, 1.0, 0, 400, shear=shear).save(p)
        tmp.append(p)
        s = SourceDescriptor()
        s.path, s.name, s.location = p, name, {"Slant": v}
        d.addSource(s)
    vf, _, _ = varlib_build(d)
    data = save(vf)
    for p in tmp:
        os.remove(p)
    return data


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "snap", "tests", "fixtures", "fonts")
    os.makedirs(out, exist_ok=True)
    var = variable(out)
    sans_reg = save(build("Test Sans", "Regular", 55, 1.0, 0, 400))
    sans_bold = save(build("Test Sans", "Bold", 95, 1.0, 40, 700, bold=True))
    ser_reg = save(build("Test Serif", "Regular", 50, 1.0, 20, 400))
    ser_bold = save(build("Test Serif", "Bold", 90, 1.0, 60, 700, bold=True))
    ser_ital = save(build("Test Serif", "Italic", 50, 1.0, 20, 400, italic=True))
    for n, b in (("TestSans-VariableFont_wdth,wght.ttf", var), ("TestSerif-Regular.ttf", ser_reg),
                 ("TestSerif-Bold.ttf", ser_bold), ("TestSlant-VariableFont_slnt.ttf", slanted(out))):
        with open(os.path.join(out, n), "wb") as f:
            f.write(b)

    def zipit(name, entries):
        with zipfile.ZipFile(os.path.join(out, name), "w", zipfile.ZIP_DEFLATED) as z:
            for n, b in entries:
                z.writestr(n, b)

    zipit("Test_Sans.zip", [
        ("OFL.txt", OFL), ("README.txt", "Test Sans fixture\n"),
        ("TestSans-VariableFont_wdth,wght.ttf", var),
        ("TestSans-Italic-VariableFont_wdth,wght.ttf", var),  # stands in for the italic file
        ("static/TestSans-Regular.ttf", sans_reg), ("static/TestSans-Bold.ttf", sans_bold),
    ])
    zipit("Test_Serif.zip", [("OFL.txt", OFL), ("static/TestSerif-Regular.ttf", ser_reg),
                             ("static/TestSerif-Bold.ttf", ser_bold), ("static/TestSerif-Italic.ttf", ser_ital)])
    zipit("Test_Plain.zip", [("TestSerif-Regular.ttf", ser_reg)])


if __name__ == "__main__":
    main()
