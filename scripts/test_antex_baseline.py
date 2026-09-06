import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location(
    "baseline", Path(__file__).with_name("antex-baseline.py")
)
BASELINE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASELINE)


class ProductionGraphTests(unittest.TestCase):
    def test_excludes_dev_edges_and_retains_build_edges_and_cycles(self):
        metadata = {
            "packages": [
                {"id": name, "name": name}
                for name in ["cli", "runtime", "build", "tests"]
            ],
            "resolve": {
                "nodes": [
                    {
                        "id": "cli",
                        "deps": [
                            {"pkg": "runtime", "dep_kinds": [{"kind": None}]},
                            {"pkg": "build", "dep_kinds": [{"kind": "build"}]},
                            {"pkg": "tests", "dep_kinds": [{"kind": "dev"}]},
                        ],
                    },
                    {
                        "id": "runtime",
                        "deps": [{"pkg": "cli", "dep_kinds": [{"kind": None}]}],
                    },
                    {"id": "build", "deps": []},
                    {"id": "tests", "deps": []},
                ]
            },
        }
        _, reached, reverse = BASELINE.production_graph(metadata, "cli")
        self.assertEqual(
            (reached, reverse),
            (
                {"cli", "runtime", "build"},
                {"runtime": {"cli"}, "build": {"cli"}, "cli": {"runtime"}},
            ),
        )

    def test_missing_root_fails_instead_of_reporting_empty_success(self):
        with self.assertRaisesRegex(ValueError, "exactly one root"):
            BASELINE.production_graph({"packages": []}, "antex-cli")

    def test_includes_companion_binary_dependencies(self):
        metadata = {
            "packages": [{"id": name, "name": name} for name in ["cli", "host", "v8"]],
            "resolve": {
                "nodes": [
                    {"id": "cli", "deps": []},
                    {
                        "id": "host",
                        "deps": [{"pkg": "v8", "dep_kinds": [{"kind": None}]}],
                    },
                    {"id": "v8", "deps": []},
                ]
            },
        }
        _, reached, reverse = BASELINE.production_graph(metadata, ["cli", "host"])
        self.assertEqual((reached, reverse), ({"cli", "host", "v8"}, {"v8": {"host"}}))


if __name__ == "__main__":
    unittest.main()
