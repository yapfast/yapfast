"""Binary packages must carry notices for embedded UI fonts."""
import importlib.util
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release_package", Path(__file__).resolve().parents[1] / "scripts/release_package.py")
release_package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_package)


class FontNoticesTest(unittest.TestCase):
    def test_packages_keep_each_font_license_and_its_attribution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fonts = root / "ui/fonts"
            fonts.mkdir(parents=True)
            notices = {"LICENSE-OFL.txt": "original font license", "LICENSE-OFL-DMSans.txt": "DMSans license", "README.md": "font provenance"}
            for name, text in notices.items():
                (fonts / name).write_text(text)
            (fonts / "DMSans-Regular.ttf").write_bytes(b"font embedded in binary")
            package = root / "package"
            package.mkdir()
            skia = root / "skia"
            skia.mkdir()
            (root / "LICENSE.md").write_text("libwebrtc license")
            archive = root / "skia.tar"
            with tarfile.open(archive, "w"):
                pass
            with patch.object(release_package, "ROOT", root):
                release_package.dependency_notices({"packages": []}, package, package / "licenses", archive, skia, root)
            copied = package / "licenses/fonts"
            self.assertEqual({p.name: p.read_text() for p in copied.iterdir()}, notices)


if __name__ == "__main__":
    unittest.main()
