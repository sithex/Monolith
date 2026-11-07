"""Monolith's cache-only adaptation; no Hermes imports, file writes or real threads."""
from pathlib import Path
import runpy
from types import SimpleNamespace
from unittest.mock import patch


def plugin():
    # Loading the hook definitions is lazy: it neither installs nor starts the plugin.
    return runpy.run_path(str(Path(__file__).resolve().parent.parent / "__init__.py"))


def test_default_fetch_cannot_contact_provider():
    mod = plugin()
    try:
        mod["_default_fetch"]("https://example.invalid", "not-a-credential")
    except RuntimeError as error:
        assert "network fetching is disabled" in str(error)
    else:
        raise AssertionError("default fetch must reject all requests")


def test_hooks_force_cache_even_with_fetch_environment():
    mod = plugin()
    ensure = mod["_start"]
    # Functions retain runpy's execution globals, rather than its returned copy.
    state = ensure.__globals__
    modes = []
    threads = []
    state["Metrics"] = lambda out: SimpleNamespace(out=out, base_urls_snapshot=[])
    state["_out_dir"] = lambda: Path("unused-metrics-test-home")
    state["PlanPoller"] = lambda out, urls, *, mode: modes.append(mode) or object()
    fake_thread = lambda **kwargs: SimpleNamespace(start=lambda: threads.append(kwargs["name"]))
    with patch.dict("os.environ", {"HABBY_METRICS_PLAN": "fetch"}), \
            patch.object(state["threading"], "Thread", fake_thread), \
            patch.object(state["atexit"], "register", lambda callback: None):
        ensure()
    assert modes == ["cache"]
    assert threads == ["habby-metrics-writer", "habby-metrics-plan"]
    assert state["_started"] and not state["_disabled"]


if __name__ == "__main__":
    test_default_fetch_cannot_contact_provider()
    test_hooks_force_cache_even_with_fetch_environment()
    print("2 cache-only plugin tests passed (no installation, Hermes imports or disk writes)")
