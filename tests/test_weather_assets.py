"""Guard embedded icons against missing, empty, or malformed SVGs."""
from pathlib import Path
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
SVG = "{http://www.w3.org/2000/svg}"


class WeatherAssetTests(unittest.TestCase):
    def test_embedded_icons_are_valid_and_contain_shapes(self):
        source = (ROOT / "src/main.rs").read_text()
        paths = re.findall(r'include_bytes!\("\.\./(assets/[^"\n]+\.svg)"\)', source)
        self.assertTrue(paths, "No embedded icons found")
        for path in paths:
            with self.subTest(path=path):
                root = ET.parse(ROOT / path).getroot()
                self.assertEqual(root.tag, SVG + "svg")
                self.assertEqual(root.get("viewBox"), "0 0 64 64")
                self.assertTrue(any(element.tag in {
                    SVG + shape for shape in ("path", "circle", "rect", "ellipse", "polygon", "polyline", "line")
                } for element in root.iter()), "SVG contains no drawable shapes")
