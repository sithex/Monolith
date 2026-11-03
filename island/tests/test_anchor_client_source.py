import importlib.util
import pathlib
import unittest

path = pathlib.Path(__file__).resolve().parents[2] / "island" / "anchor-client.py"
spec = importlib.util.spec_from_file_location("monitor_anchor", path)
client = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client)

class SourceArgumentTests(unittest.TestCase):
    def test_source_id_is_preserved_for_pin_and_launch(self):
        self.assertEqual(client.parse_args(["pin", "42"]), ("pin", " 42"))
        self.assertEqual(client.parse_args(["launch", "42"]), ("launch", " 42"))
        self.assertEqual(client.parse_args(["pin"]), ("pin", ""))
    def test_rejects_injection_wrong_operation_and_invalid_ids(self):
        for args in [[],["status","42"],["pin","0"],["pin","-1"],["pin","4294967296"],["pin","２"],["pin","2;quit"],["pin","2\n"]]:
            with self.subTest(args=args), self.assertRaises(ValueError): client.parse_args(args)
