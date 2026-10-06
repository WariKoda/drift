#!/usr/bin/env python3
"""Assemble native, test-only GUI archives; never build, install, or publish them."""

import argparse
import gzip
import hashlib
import io
import os
from pathlib import Path, PurePosixPath
import platform
import plistlib
import re
import selectors
import shutil
import signal
import stat
import struct
import subprocess
import tarfile
import tempfile
import time


TARGETS = {
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64"),
    "x86_64-apple-darwin": ("Darwin", "x86_64"),
    "aarch64-apple-darwin": ("Darwin", "arm64"),
}
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
DESKTOP = b"""[Desktop Entry]
Type=Application
Name=Drift GUI
Exec=drift-gui
TryExec=drift-gui
Terminal=false
Categories=Development;Network;
"""


class PackagingError(ValueError):
    pass


def parse_version(version):
    """Return the numeric core, retaining the original version elsewhere."""
    if not isinstance(version, str) or not 1 <= len(version) <= 128:
        raise PackagingError("version must be SemVer of at most 128 characters")
    match = SEMVER.fullmatch(version)
    if match is None or any(int(part) > 65535 for part in match.group(1, 2, 3)):
        raise PackagingError("version must be non-v-prefixed SemVer with core components <= 65535")
    if any(
        part.isdigit() and len(part) > 1 and part[0] == "0"
        for part in (match.group(4) or "").split(".")
    ):
        raise PackagingError("numeric prerelease identifiers must not have leading zeros")
    return ".".join(match.group(1, 2, 3))


def parse_epoch(value):
    if value is None:
        return 0
    # gzip stores its timestamp as an unsigned 32-bit integer.
    if not re.fullmatch(r"[0-9]{1,10}", value) or int(value) > 0xFFFFFFFF:
        raise PackagingError("SOURCE_DATE_EPOCH must be a nonnegative 32-bit integer")
    return int(value)


def require_native_target(target, system, machine):
    if target not in TARGETS:
        raise PackagingError("unsupported target")
    if (system, machine) != TARGETS[target]:
        raise PackagingError("target must match the current native host; cross-architecture execution is forbidden")


def inspect_header(stream, size, target):
    """Check executable header layout and table bounds, without running a loader."""
    if target not in TARGETS:
        raise PackagingError("unsupported target")
    stream.seek(0)
    if target == "x86_64-unknown-linux-gnu":
        header = stream.read(64)
        if len(header) != 64:
            raise PackagingError("invalid ELF64 header")
        ident, kind, machine, version, _, phoff, shoff, _, ehsize, phsize, phnum, shsize, shnum, shstr = struct.unpack(
            "<16sHHIQQQIHHHHHH", header
        )
        if (ident[:7] != b"\x7fELF\x02\x01\x01" or machine != 62
                or kind not in (2, 3) or version != 1 or ehsize != 64):
            raise PackagingError("expected a little-endian x86_64 ELF64 executable")
        if (phnum == 0 or phnum == 0xFFFF or phsize != 56 or phoff < 64
                or phoff + phsize * phnum > size):
            raise PackagingError("invalid ELF64 program header table")
        if shnum:
            if shsize != 64 or shoff < 64 or shoff + shsize * shnum > size or shstr >= shnum:
                raise PackagingError("invalid ELF64 section header table")
        elif shoff or shstr:
            raise PackagingError("extended ELF64 section numbering is unsupported")
    else:
        header = stream.read(32)
        if len(header) != 32:
            raise PackagingError("invalid Mach-O64 header")
        magic, cpu, _, kind, ncmds, cmdbytes, _, reserved = struct.unpack("<IiiIIIII", header)
        expected_cpu = 0x01000007 if target == "x86_64-apple-darwin" else 0x0100000C
        if magic != 0xFEEDFACF or cpu != expected_cpu or kind != 2 or reserved != 0:
            raise PackagingError("expected a thin little-endian Mach-O64 executable for the target CPU")
        if ncmds == 0 or cmdbytes < ncmds * 8 or 32 + cmdbytes > size:
            raise PackagingError("invalid Mach-O64 load command table")
        offset = 32
        for _ in range(ncmds):
            stream.seek(offset)
            command = stream.read(8)
            if len(command) != 8:
                raise PackagingError("truncated Mach-O64 load command")
            _, length = struct.unpack("<II", command)
            if length < 8 or length % 8 or offset + length > 32 + cmdbytes:
                raise PackagingError("invalid Mach-O64 load command layout")
            offset += length
        if offset != 32 + cmdbytes:
            raise PackagingError("invalid Mach-O64 load command count")


def isolated_environment(home):
    env = {"PATH": os.defpath, "LANG": "C", "LC_ALL": "C", "HOME": str(home)}
    for name in ("CONFIG", "DATA", "CACHE", "STATE", "RUNTIME"):
        directory = home / name.lower()
        directory.mkdir(mode=0o700)
        env["XDG_" + name + "_HOME" if name != "RUNTIME" else "XDG_RUNTIME_DIR"] = str(directory)
    # A fresh environment also excludes display, drift logging, and loader overrides.
    return env


def bounded_command(argv, env, cwd):
    """Capture at most 64 KiB total, with a 20-second deadline; never echo replies."""
    if not hasattr(os, "waitid") or not hasattr(os, "WNOWAIT"):
        raise PackagingError("native inspection requires Python 3.13+ with waitid/WNOWAIT support")
    process = None
    try:
        process = subprocess.Popen(
            argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env=env, cwd=cwd, start_new_session=True,
        )
        output = bytearray()
        total = 0
        deadline = time.monotonic() + 20
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ, True)
            selector.register(process.stderr, selectors.EVENT_READ, False)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise PackagingError("inspection command exceeded its time limit")
                for key, _ in selector.select(remaining):
                    chunk = os.read(key.fileobj.fileno(), 8192)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    total += len(chunk)
                    if total > 65536:
                        raise PackagingError("inspection command exceeded its output limit")
                    if key.data:
                        output.extend(chunk)
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise PackagingError("inspection command exceeded its time limit")
                status = os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOWAIT | os.WNOHANG)
                if status is not None:
                    if status.si_code != os.CLD_EXITED or status.si_status != 0:
                        raise PackagingError("inspection command failed")
                    break
                time.sleep(min(0.01, remaining))
        return bytes(output)
    except (OSError, subprocess.TimeoutExpired):
        raise PackagingError("inspection command could not complete") from None
    finally:
        if process is not None:
            # WNOWAIT retains the child's PID until group cleanup, so killpg cannot
            # target a reused group ID after successful inspection.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            process.stdout.close()
            process.stderr.close()


def linux_inventory(binary, env, home):
    tool = shutil.which("readelf", path=os.defpath)
    if tool is None:
        return "Dependency inventory unavailable: readelf was not found."
    try:
        programs = bounded_command([tool, "-lW", str(binary)], env, home).decode("ascii", "replace")
        dynamic = bounded_command([tool, "-dW", str(binary)], env, home).decode("ascii", "replace")
    except PackagingError:
        return "Dependency inventory unavailable: bounded readelf inspection did not complete."
    interpreters = re.findall(r"\[Requesting program interpreter: ([A-Za-z0-9_./+@-]+)\]", programs)
    needed = sorted(set(re.findall(r"\(NEEDED\).*Shared library: \[([A-Za-z0-9_./+@-]+)\]", dynamic)))
    return (
        "Inspection tool: readelf (no loader execution).\n"
        + "ELF interpreter: " + (", ".join(interpreters) or "none reported") + "\n"
        + "DT_NEEDED: " + (", ".join(needed) or "none reported")
        + "\nThis inventory is not a recursive dependency or compatibility check."
    )


def archive_contents(version, target, license_bytes, inventory):
    core = parse_version(version)
    if target not in TARGETS:
        raise PackagingError("unsupported target")
    readme = (
        f"Drift GUI {version} ({target})\n\n"
        "TEST ONLY: experimental GUI artifacts, not a production release.\n"
        "SSH dependency, Monokai parity, and native GUI validation gates remain open.\n"
        "These are manual CI test artifacts only; no publishing or tags.\n"
        "Git on PATH is required for repository discovery and gitignore classification.\n"
    )
    if target == "x86_64-unknown-linux-gnu":
        readme += (
            "\nLinux x86_64 only. No installer is included. Manual placement on PATH\n"
            "and in XDG application directories is required; see RUNTIME.md.\n"
            "The desktop launcher supports neither file nor URL opening.\n"
        )
        runtime = (
            "TEST ONLY runtime notes\n\n"
            "Manual CI baseline: Ubuntu 24.04 x86_64. Local builds inherit their host ABI.\n"
            "Dynamic libraries are not bundled; glibc requirements depend on the build host.\n"
            "Git and usable display/GPU/backend libraries and fonts are supplied separately.\n"
            "Use the target system's ELF interpreter and readelf to inspect requirements;\n"
            "never use ldd on an untrusted artifact. No Linux portability is claimed.\n"
            "SSH dependency, Monokai parity, and native GUI validation gates remain open.\n"
            "A successful version check does not validate GUI launch or remote operations.\n\n"
            + inventory + "\n"
        )
        return {
            "bin/drift-gui": (0o755, None),
            "share/applications/io.github.WariKoda.drift-gui.desktop": (0o644, DESKTOP),
            "LICENSE": (0o644, license_bytes),
            "README.md": (0o644, readme.encode()),
            "RUNTIME.md": (0o644, runtime.encode()),
        }
    readme += (
        "\nSeparate thin Intel/ARM bundles, not a universal application. macOS 15.0+.\n"
        "No Developer ID signing or notarization; no codesign or Gatekeeper bypass\n"
        "is performed by this assembler. The toolchain may ad-hoc sign Mach-O\n"
        "automatically. Gatekeeper may block this test application.\n"
        "Native macOS CI and interactive .app launch checks are still required;\n"
        "Linux assembly tests do not establish that an .app launches.\n"
        "No macOS OpenFile events, document types, or URL handlers are supported.\n"
        "Bundle versions use the numeric core for TEST artifacts, not App Store increments.\n"
    )
    info = {
        "CFBundleIdentifier": "io.github.WariKoda.drift-gui",
        "CFBundleName": "Drift GUI",
        "CFBundleDisplayName": "Drift GUI",
        "CFBundleExecutable": "drift-gui",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": core,
        "CFBundleVersion": core,
        "DriftBuildVersion": version,
        "LSMinimumSystemVersion": "15.0",
        "NSHighResolutionCapable": True,
        "NSPrincipalClass": "NSApplication",
    }
    return {
        "Drift GUI.app/Contents/MacOS/drift-gui": (0o755, None),
        "Drift GUI.app/Contents/Info.plist": (0o644, plistlib.dumps(info, fmt=plistlib.FMT_XML, sort_keys=True)),
        "Drift GUI.app/Contents/Resources/LICENSE": (0o644, license_bytes),
        "README.md": (0o644, readme.encode()),
    }


def write_archive(destination, binary, version, target, epoch, license_bytes, inventory):
    """Structural assembly seam; the CLI always validates a staged binary first."""
    contents = archive_contents(version, target, license_bytes, inventory)
    parse_epoch(str(epoch))
    top = f"drift-gui-{version}-{target}"
    entries = {top: (0o755, b"", True)}
    for relative, (mode, data) in contents.items():
        name = PurePosixPath(top, relative)
        entries[str(name)] = (mode, data, False)
        for parent in name.parents:
            if str(parent) != ".":
                entries[str(parent)] = (0o755, b"", True)
    with destination.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                for name in sorted(entries):
                    mode, data, directory = entries[name]
                    info = tarfile.TarInfo(name)
                    info.mode, info.mtime = mode, epoch
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    if directory:
                        info.type = tarfile.DIRTYPE
                        archive.addfile(info)
                    elif data is None:
                        with binary.open("rb") as source:
                            info.size = os.fstat(source.fileno()).st_size
                            archive.addfile(info, source)
                    else:
                        info.size = len(data)
                        archive.addfile(info, io.BytesIO(data))
        raw.flush()
        os.fsync(raw.fileno())
    destination.chmod(0o644)


def publish_archive(staged, output, basename):
    """No-clobber hard links; the final archive is the last completion marker."""
    archive = output / basename
    checksum = output / (basename + ".sha256")
    if os.path.lexists(archive) or os.path.lexists(checksum):
        raise PackagingError("archive or checksum already exists; refusing overwrite")
    digest = hashlib.sha256()
    with staged.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    sidecar = staged.with_name("checksum.sha256.partial")
    with sidecar.open("xb") as stream:
        stream.write(f"{digest.hexdigest()}  {basename}\n".encode("ascii"))
        stream.flush()
        os.fsync(stream.fileno())
    sidecar.chmod(0o644)
    linked = []
    try:
        # A crash between these links leaves only a checksum, never a complete-looking archive.
        os.link(sidecar, checksum)
        linked.append(checksum)
        os.link(staged, archive)
        linked.append(archive)
        directory_fd = os.open(output, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    except OSError:
        for path in reversed(linked):
            path.unlink()
        raise PackagingError("could not publish archive and checksum without overwriting") from None
    return archive


def package_binary(binary, version, target, output, epoch):
    parse_version(version)
    parse_epoch(str(epoch))
    require_native_target(target, platform.system(), platform.machine())
    basename = f"drift-gui-{version}-{target}.tar.gz"
    output.mkdir(parents=True, exist_ok=True)
    if os.path.lexists(output / basename) or os.path.lexists(output / (basename + ".sha256")):
        raise PackagingError("archive or checksum already exists; refusing overwrite")
    with tempfile.TemporaryDirectory(prefix=".drift-gui-stage-", dir=output) as temporary:
        stage = Path(temporary).resolve()
        snapshot = stage / "drift-gui"
        metadata = binary.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size == 0:
            raise PackagingError("binary must be a nonempty regular file, not a symlink")
        fd = os.open(binary, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, "rb") as source:
            metadata = os.fstat(source.fileno())
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_size == 0:
                raise PackagingError("binary must be a nonempty regular file, not a symlink")
            with snapshot.open("xb") as destination:
                shutil.copyfileobj(source, destination)
        with snapshot.open("rb") as stream:
            inspect_header(stream, snapshot.stat().st_size, target)
        snapshot.chmod(0o755)
        home = stage / "home"
        home.mkdir(mode=0o700)
        env = isolated_environment(home)
        reply = bounded_command([str(snapshot.resolve()), "version"], env, home)
        if reply != f"drift-gui {version}\n".encode("ascii"):
            raise PackagingError("binary version output does not exactly match the requested version")
        inventory = linux_inventory(snapshot.resolve(), env, home) if target.endswith("linux-gnu") else ""
        license_bytes = (Path(__file__).resolve().parents[2] / "LICENSE").read_bytes()
        staged_archive = stage / "archive.tar.gz.partial"
        write_archive(staged_archive, snapshot, version, target, epoch, license_bytes, inventory)
        return publish_archive(staged_archive, output, basename)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True, choices=tuple(TARGETS))
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        archive = package_binary(
            args.binary, args.version, args.target, args.output, parse_epoch(os.environ.get("SOURCE_DATE_EPOCH")),
        )
    except PackagingError as error:
        parser.exit(1, f"error: {error}\n")
    except OSError:
        parser.exit(1, "error: packaging file operation failed\n")
    print(archive.name)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
