"""Offline regression: validate the Python SDK end-to-end against a local
file:// fixture. Uses the real chrome-headless-shell (cached at first run via
the CFT cache — same one `vakd doctor` / `cft` uses), so this is a true
smoke test of the ctypes binding + request model."""
import os
import pathlib

import pytest

from vakbrowse import Session, VakError

ROOT = pathlib.Path(__file__).resolve().parents[2]
FORM = f"file://{ROOT}/tests/fixtures/form.html"


@pytest.fixture(scope="module")
def session():
    s = Session()
    sid, _url = s.open(FORM)
    yield s, sid
    s.close(sid)


def test_ffi_version_nonempty():
    from vakbrowse import get_lib
    lib = get_lib()
    assert hasattr(lib, "vak_version")


def test_open_and_snapshot(session):
    s, sid = session
    snap = s.snapshot(sid)
    assert snap["url"].endswith("form.html")
    names = [e["name"] for e in snap["elements"]]
    assert any("name" in n.lower() for n in names)


def test_fill_and_wait(session):
    s, sid = session
    snap = s.snapshot(sid)
    name_ref = next(
        e["ref"] for e in snap["elements"]
        if e["role"] == "textbox" and "name" in e["name"].lower()
    )
    s.fill(sid, name_ref, "Linus")
    # Submit the form (fill alone never triggers the #out write).
    btn = next(e["ref"] for e in snap["elements"] if e["role"] == "button")
    s.click(sid, btn)
    # The fixture updates #out via JS on submit (no URL change), so wait on
    # the DOM expression directly — mirrors the CLI `wait` action.
    s.act(sid, {
        "type": "wait_for_truthy",
        "expression": "document.getElementById('out').textContent.includes('name=Linus')",
        "timeout_ms": 2000,
    })
    out = s.act(sid, {"type": "eval_text",
                      "expression": "document.getElementById('out').textContent"})["text"]
    assert "name=Linus" in out, out


def test_batch_runs_actions_in_one_roundtrip(session):
    s, sid = session
    results = s.batch(sid, [
        {"type": "snapshot"},
        {"type": "extract"},
    ])
    assert len(results) == 2
    assert results[0]["type"] == "snapshot"
    assert results[1]["type"] == "text"


def test_rotate_proxy_without_pool_is_error(session):
    s, sid = session
    with pytest.raises(VakError, match="proxy pool"):
        s.rotate_proxy(sid)


def test_list_sessions_roundtrips(session):
    s, sid = session
    sessions = s.sessions()
    assert any(sid == info["id"] for info in sessions)


def test_source_returns_html(session):
    s, sid = session
    src = s.source(sid)
    assert "<html" in src.lower()
    assert "</html>" in src.lower()
    assert "Linus" not in src  # form not yet filled


def test_click_returns_navigation_info(session):
    s, sid = session
    snap = s.snapshot(sid)
    btn_ref = next(
        e["ref"] for e in snap["elements"]
        if e["role"] == "button" and "send" in e["name"].lower()
    )
    result = s.click(sid, btn_ref)
    assert result["type"] == "clicked"
    # JS handler prevents default → navigated:false on the form fixture.
    assert "navigated" in result


def test_find_by_css_returns_refs(session):
    s, sid = session
    # The form fixture has an input[type=text] and a button.
    refs = s.find(sid, "input[type=text]")
    assert len(refs) >= 1
    assert all(r.startswith("@e") for r in refs)


def test_eval_returns_stringified(session):
    s, sid = session
    val = s.eval(sid, "1 + 2")
    assert val == "3"
    flag = s.eval(sid, "navigator.webdriver")
    assert flag == "false" or flag == "true"  # truthy or falsy, either is fine


def test_downloads_returns_list(session):
    s, sid = session
    downloads = s.downloads(sid)
    assert isinstance(downloads, list)
