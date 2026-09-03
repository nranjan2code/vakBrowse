"""Setup: build (if needed) and bundle the vakBrowse FFI cdylib.

A Rust toolchain is required (`cargo build -p vakbrowse-ffi --release` runs
inside `crates/vakbrowse-ffi`, two levels up from this file). The resulting
`libvakbrowse_ffi.{dylib,so,dll}` is copied into the `vakbrowse` package so
the wheel is self-contained and importable without `cargo` on the target.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

from setuptools import setup
from setuptools.command.build_py import build_py

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
CRATES = REPO / "crates" / "vakbrowse-ffi"
CARGO_TARGET = REPO / "target"

if sys.platform == "darwin":
    LIB_NAME = "libvakbrowse_ffi.dylib"
elif os.name == "nt":
    LIB_NAME = "vakbrowse_ffi.dll"
else:
    LIB_NAME = "libvakbrowse_ffi.so"


def built_lib() -> Path:
    return CARGO_TARGET / "release" / LIB_NAME


def ensure_built() -> Path:
    lib = built_lib()
    if not lib.exists():
        subprocess.run(
            [
                "cargo", "build", "-p", "vakbrowse-ffi", "--release",
                # `strip=true` in profile.release (keeps executables slim)
                # corrupts a *cdylib*'s __LINKEDIT alignment on macOS, so dyld
                # rejects the result ("mis-aligned LINKEDIT string pool").
                # Disable stripping for the FFI shared lib only.
                "--config", "profile.release.strip=false",
            ],
            cwd=REPO, check=True,
        )
    return lib


class build_py_with_lib(build_py):
    def run(self) -> None:
        lib = ensure_built()
        dest = Path(self.build_lib) / "vakbrowse" / LIB_NAME
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(lib, dest)
        super().run()


setup(cmdclass={"build_py": build_py_with_lib})
