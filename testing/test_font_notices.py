"""Binary packages must carry notices for embedded artwork and fonts."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "release_package", Path(__file__).resolve().parents[1] / "scripts/release_package.py"
)
if spec is None or spec.loader is None:
    raise SystemExit("could not load scripts/release_package.py")
release_package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_package)


class FontNoticesTest(unittest.TestCase):
    def test_packages_keep_the_embedded_assets_licenses_and_attribution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            asset_licenses = root / "assets/licenses"
            fonts = asset_licenses / "fonts"
            fonts.mkdir(parents=True)
            notices = {
                "display-names/LICENSE-OFL-DisplayNames.txt": "display name fonts' license",
                "dm-sans/LICENSE-OFL-DMSans.txt": "DMSans license",
                "atkinson-hyperlegible-next/LICENSE-OFL-Atkinson.txt": "Atkinson license",
            }
            for name, text in notices.items():
                path = fonts / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            font_assets = root / "assets/fonts/dm-sans"
            font_assets.mkdir(parents=True)
            (font_assets / "DMSans-Regular.ttf").write_bytes(b"font embedded in binary")
            package = root / "package"
            package.mkdir()
            (root / "LICENSE.md").write_text("libwebrtc license")
            (asset_licenses / "LICENSE-emojibase.txt").write_text("emojibase license")
            attribution = root / "docs/third-party"
            attribution.mkdir(parents=True)
            (attribution / "fonts.md").write_text("font provenance")
            (attribution / "icons.md").write_text("icon provenance")
            (attribution / "twemoji.md").write_text("Twemoji provenance")
            icons = asset_licenses / "icons"
            icons.mkdir(parents=True)
            (icons / "LICENSE").write_text("Lucide license")
            twemoji = root / "assets/twemoji"
            twemoji.mkdir(parents=True)
            (asset_licenses / "twemoji").mkdir()
            (asset_licenses / "twemoji/LICENSE-GRAPHICS").write_text("Twemoji graphics license")
            (twemoji / "REVISION").write_text("revision")
            licenses = package / "licenses"
            with patch.object(release_package, "ROOT", root):
                dependencies = release_package.dependency_notices(
                    {"packages": []}, package, licenses, root
                )
            copied = licenses / "fonts"
            self.assertEqual(
                {
                    p.relative_to(copied).as_posix(): p.read_text()
                    for p in copied.rglob("*")
                    if p.is_file()
                },
                {**notices, "ATTRIBUTION.md": "font provenance"},
            )
            self.assertEqual((licenses / "icons/LICENSE").read_text(), "Lucide license")
            self.assertEqual((licenses / "icons/ATTRIBUTION.md").read_text(), "icon provenance")
            twemoji_notices = {
                name: (package / name).read_text() for name in dependencies[0]["license_files"]
            }
            self.assertEqual(
                twemoji_notices,
                {
                    "licenses/twemoji-LICENSE-GRAPHICS": "Twemoji graphics license",
                    "licenses/twemoji-ATTRIBUTION.md": "Twemoji provenance",
                },
            )


if __name__ == "__main__":
    unittest.main()
