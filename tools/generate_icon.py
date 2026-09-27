"""Generate Byakko's original geometric B mark (no font dependency).

Requires Pillow only; generated files are checked in, so builds need no Python.
"""
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent / "packaging" / "icons"
ROOT.mkdir(parents=True, exist_ok=True)
# Cubic contours in a 256-unit square. The geometric sans-serif letter is
# original artwork, rather than an embedded or redistributed commercial font.
CONTOURS = [
    [(72, 44), (126, 44), ((169, 44), (190, 62), (190, 96)),
     ((190, 113), (181, 127), (166, 134)),
     ((187, 141), (198, 154), (198, 174)),
     ((198, 201), (177, 216), (138, 216)), (72, 216)],
    [(102, 72), (102, 120), (127, 120),
     ((148, 120), (160, 112), (160, 96)),
     ((160, 80), (149, 72), (127, 72))],
    [(102, 148), (102, 188), (137, 188),
     ((157, 188), (167, 182), (167, 169)),
     ((167, 155), (156, 148), (137, 148))],
]

def polygon(contour):
    points = [contour[0]]
    for segment in contour[1:]:
        if len(segment) == 2:
            points.append(segment)
        else:
            start = points[-1]
            a, b, end = segment
            for step in range(1, 65):
                t = step / 64
                points.append(tuple((1-t)**3 * start[i] + 3*(1-t)**2*t*a[i]
                                    + 3*(1-t)*t*t*b[i] + t**3*end[i]
                                    for i in (0, 1)))
    return [(round(x*4), round(y*4)) for x, y in points]

canvas = Image.new("RGBA", (1024, 1024), "#595bea")
draw = ImageDraw.Draw(canvas)
for index, contour in enumerate(CONTOURS):
    draw.polygon(polygon(contour), fill="white" if index == 0 else "#595bea")
icon = canvas.resize((256, 256), Image.Resampling.LANCZOS)
icon.save(ROOT / "byakko.png")
icon.save(ROOT / "byakko.ico", sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)])
(ROOT / "byakko.rgba").write_bytes(icon.resize((64, 64), Image.Resampling.LANCZOS).tobytes())
paths = []
for contour in CONTOURS:
    path = f"M{contour[0][0]} {contour[0][1]}"
    for segment in contour[1:]:
        path += (f" L{segment[0]} {segment[1]}" if len(segment) == 2 else
                 " C" + " ".join(f"{x} {y}" for x, y in segment))
    paths.append(path + " Z")
(ROOT / "byakko.svg").write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">\n'
    '<rect width="256" height="256" fill="#595bea"/>\n'
    f'<path fill="white" fill-rule="evenodd" d="{" ".join(paths)}"/>\n</svg>\n',
    encoding="utf-8")
