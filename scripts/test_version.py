import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location("version", Path(__file__).with_name("version.py"))
VERSION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERSION)


class VersionTest(unittest.TestCase):
    def test_semver_bumps_do_not_change_dependency_versions(self):
        self.assertEqual(VERSION.bump("0.0.0", "patch"), "0.0.1")
        self.assertEqual(VERSION.bump("1.2.3", "minor"), "1.3.0")
        self.assertEqual(VERSION.bump("1.2.3", "major"), "2.0.0")
        manifest = '[workspace.package]\nversion = "0.0.0"\nfoo = "0.0.0"\n'
        self.assertEqual(
            VERSION.replace_manifest(manifest, "0.0.0", "0.0.1"),
            '[workspace.package]\nversion = "0.0.1"\nfoo = "0.0.0"\n',
        )
        packages = "".join(
            f'[[package]]\nname = "antex-{index}"\nversion = "0.0.0"\n\n'
            for index in range(7)
        )
        packages += '[[package]]\nname = "external"\nversion = "0.0.0"\n'
        updated = VERSION.replace_lock(packages, "0.0.0", "0.0.1")
        self.assertIn('name = "external"\nversion = "0.0.0"', updated)
        self.assertEqual(updated.count('version = "0.0.1"'), 7)


if __name__ == "__main__":
    unittest.main()
