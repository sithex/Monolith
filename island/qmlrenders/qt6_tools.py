"""Find a Qt 6 tool (qmltestrunner, qml) on Arch and Fedora.

Arch keeps them in /usr/lib/qt6/bin, Fedora (x86_64) in /usr/lib64/qt6/bin, and Fedora's
PATH has them as `<name>-qt6`. The bare name is never used: on Fedora `/usr/bin/qmltestrunner`
and `/usr/bin/qml` are Qt 5's, and on Arch `/usr/bin/qmltestrunner` is a wrapper that exits 1.
"""
import os
import shutil

QT6_DIRS = ("/usr/lib/qt6/bin", "/usr/lib64/qt6/bin")


def _executable(path):
    return os.path.isfile(path) and os.access(path, os.X_OK)


def candidates(name):
    return [os.path.join(d, name) for d in QT6_DIRS]


def find(name, is_executable=_executable, which=shutil.which):
    """Path of the Qt 6 `name`, or None."""
    for path in candidates(name):
        if is_executable(path):
            return path
    for alias in (name + "-qt6", name + "6"):
        found = which(alias)
        if found:
            return found
    return None


def require(name):
    """Path of the Qt 6 `name`; exits with a package hint when it is missing."""
    found = find(name)
    if found is None:
        raise SystemExit(
            f"Qt 6 {name} not found in {', '.join(QT6_DIRS)} or as {name}-qt6/{name}6 on PATH "
            "(Arch: qt6-declarative, Fedora: qt6-qtdeclarative-devel)")
    return found
