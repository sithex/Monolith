"""python3 -m unittest discover -s island/qmlrenders -p 'test_*.py'"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qt6_tools  # noqa: E402


def fake(files, path_names=()):
    """An is_executable/which pair over a fake filesystem."""
    files = set(files)
    on_path = {name: "/usr/bin/" + name for name in path_names}
    return (lambda p: p in files), on_path.get


class FindTest(unittest.TestCase):
    def test_arch_unchanged(self):
        exe, which = fake({"/usr/lib/qt6/bin/qmltestrunner", "/usr/lib/qt6/bin/qml"},
                          ("qmltestrunner", "qml", "qml6"))
        self.assertEqual(qt6_tools.find("qmltestrunner", exe, which), "/usr/lib/qt6/bin/qmltestrunner")
        self.assertEqual(qt6_tools.find("qml", exe, which), "/usr/lib/qt6/bin/qml")

    def test_fedora_lib64_never_bare_qt5(self):
        exe, which = fake({"/usr/lib64/qt6/bin/qmltestrunner", "/usr/bin/qmltestrunner"},
                          ("qmltestrunner", "qmltestrunner-qt6"))
        self.assertEqual(qt6_tools.find("qmltestrunner", exe, which), "/usr/lib64/qt6/bin/qmltestrunner")

    def test_path_aliases_in_order(self):
        exe, which = fake(set(), ("qml", "qml-qt6", "qml6"))
        self.assertEqual(qt6_tools.find("qml", exe, which), "/usr/bin/qml-qt6")
        exe, which = fake(set(), ("qml", "qml6"))
        self.assertEqual(qt6_tools.find("qml", exe, which), "/usr/bin/qml6")

    def test_only_qt5_bare_name_is_not_found(self):
        exe, which = fake({"/usr/bin/qmltestrunner"}, ("qmltestrunner",))
        self.assertIsNone(qt6_tools.find("qmltestrunner", exe, which))

    def test_require_names_the_packages(self):
        with self.assertRaises(SystemExit) as raised:
            qt6_tools.require("no-such-qt-tool-xyz")
        self.assertIn("qt6-qtdeclarative-devel", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
