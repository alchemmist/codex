import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class InstallerTest(unittest.TestCase):
    def test_installs_exactly_one_versioned_binary(self):
        if subprocess.check_output(["uname", "-s"], text=True).strip() != "Darwin":
            self.skipTest("host-specific installer mapping")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            release = root / "release"
            release.mkdir()
            binary = root / "antex"
            binary.write_text("binary")
            archive = release / "antex-aarch64-apple-darwin.tar.gz"
            with tarfile.open(archive, "w:gz") as output:
                output.add(binary, arcname="antex")
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            (release / f"{archive.name}.sha256").write_text(f"{digest}  {archive.name}\n")
            install = root / "bin"
            environment = os.environ.copy()
            environment.update(
                {
                    "ANTEX_VERSION": "0.0.1",
                    "ANTEX_RELEASE_BASE_URL": release.as_uri(),
                    "ANTEX_INSTALL_DIR": str(install),
                }
            )
            result = subprocess.run(
                [str(ROOT / "scripts/install-antex-release.sh"), "mac"],
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((install / "antex").read_text(), "binary")
            self.assertEqual([path.name for path in install.iterdir()], ["antex"])


if __name__ == "__main__":
    unittest.main()
