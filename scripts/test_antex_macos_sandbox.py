import json
import pathlib
import shlex
import subprocess
import sys
import tempfile
import unittest


POLICY = (pathlib.Path(__file__).resolve().parents[1] / "antex-rs/runtime/src/seatbelt_base.sbpl").read_text()


def execute(workspace, script):
    with tempfile.TemporaryDirectory(prefix="antex-sandbox-probe-") as temporary:
        temporary = str(pathlib.Path(temporary).resolve())
        policy = POLICY
        for root in ["/System", "/usr", "/bin", "/sbin", "/private/etc", "/Library/Apple", workspace]:
            policy += f"(allow file-read* file-map-executable (subpath {json.dumps(root)}))"
        for root in [temporary, workspace]:
            policy += f"(allow file-read* file-write* file-map-executable (subpath {json.dumps(root)}))"
        return subprocess.run(
            ["/usr/bin/sandbox-exec", "-p", policy, "/bin/sh", "-c", script],
            cwd=workspace, capture_output=True, text=True, timeout=5,
            env={"PATH": "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin", "HOME": temporary,
                 "TMPDIR": temporary, "LANG": "C.UTF-8", "TERM": "dumb"},
            start_new_session=True,
        )


@unittest.skipUnless(sys.platform == "darwin", "requires macOS Seatbelt")
class SeatbeltTest(unittest.TestCase):
    def test_pwd_runs_in_temporary_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = str(pathlib.Path(directory).resolve())
            result = execute(workspace, "pwd")
            self.assertEqual((result.returncode, result.stdout, result.stderr), (0, workspace + "\n", ""))

    def test_pwd_runs_in_home_workspace(self):
        workspace = str(pathlib.Path.home())
        result = execute(workspace, "pwd")
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, workspace + "\n", ""))

    def test_root_directory_rule_does_not_grant_read_access_to_unrelated_files(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as outside:
            secret = pathlib.Path(outside).resolve() / "secret"
            secret.write_text("unrelated private data")
            result = execute(str(pathlib.Path(workspace).resolve()), "/bin/cat " + shlex.quote(str(secret)))
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, "")
            self.assertIn("Operation not permitted", result.stderr)


if __name__ == "__main__":
    unittest.main()
