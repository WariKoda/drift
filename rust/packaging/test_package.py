"""Data-only assembler tests. Native drift-gui execution is a separate CI gate."""

import contextlib
import gzip
import hashlib
import io
import os
from pathlib import Path
import platform
import plistlib
import stat
import struct
import sys
import tarfile
import tempfile
import unittest

import package


LINUX = "x86_64-unknown-linux-gnu"
INTEL_MAC = "x86_64-apple-darwin"
ARM_MAC = "aarch64-apple-darwin"
VERSION = "1.2.3-rc.1+build.7"


def elf_data(**changes):
    fields = {
        "ident": b"\x7fELF\x02\x01\x01" + bytes(9),
        "kind": 3, "machine": 62, "version": 1, "entry": 0,
        "phoff": 64, "shoff": 0, "flags": 0, "ehsize": 64,
        "phsize": 56, "phnum": 1, "shsize": 0, "shnum": 0, "shstr": 0,
    }
    fields.update(changes)
    return struct.pack("<16sHHIQQQIHHHHHH", *fields.values()) + struct.pack(
        "<IIQQQQQQ", 1, 5, 0, 0, 0, 120, 120, 4096,
    )


def macho_data(cpu=0x01000007, **changes):
    fields = {
        "magic": 0xFEEDFACF, "cpu": cpu, "subtype": 3, "kind": 2,
        "ncmds": 1, "cmdbytes": 24, "flags": 0, "reserved": 0,
    }
    fields.update(changes)
    return struct.pack("<IiiIIIII", *fields.values()) + struct.pack("<II", 0x1B, 24) + bytes(16)


class VersionTests(unittest.TestCase):
    def test_valid_versions(self):
        for version, core in (
            ("0.0.0", "0.0.0"), ("65535.65535.65535", "65535.65535.65535"),
            (VERSION, "1.2.3"), ("1.2.3-alpha-beta.0+x-y", "1.2.3"),
            ("1.2.3-01a+01b", "1.2.3"),
            ("1.2.3+01", "1.2.3"), ("1.2.3+build.00", "1.2.3"),
            ("1.2.3-" + "a" * 122, "1.2.3"),
        ):
            with self.subTest(version=version):
                self.assertEqual(package.parse_version(version), core)

    def test_invalid_and_path_like_versions(self):
        versions = (
            "", "v1.2.3", "1.2", "1.2.3.4", "01.2.3", "1.02.3", "1.2.03",
            "65536.0.0", "0.65536.0", "0.0.65536", "999999999999999999999999.0.0",
            "1.2.3-01", "1.2.3-alpha.00",
            "1.2.3-", "1.2.3+", "1.2.3-a..b",
            "1.2.3+a..b", "1.2.3+a+b", "../1.2.3", "/1.2.3", "1.2.3/a",
            "1.2.3\\a", "1.2.3\n", "1.2.3\r", "1.2.3\x00", "1.2.3\x1b",
            " 1.2.3", "1.2.3 ", "１.2.3", "1.2.3-ä", "1.2.3-" + "a" * 123,
            "1.2.3-..", "1.2.3+../escape",
        )
        for version in versions:
            with self.subTest(version=repr(version)):
                with self.assertRaises(package.PackagingError):
                    package.parse_version(version)

    def test_epoch(self):
        for text, epoch in ((None, 0), ("0", 0), ("0001", 1), ("1700000000", 1700000000), ("4294967295", 4294967295)):
            self.assertEqual(package.parse_epoch(text), epoch)
        for text in ("", "-1", "+1", "1.0", " 1", "1\n", "１", "4294967296", "0" * 11):
            with self.subTest(text=text), self.assertRaises(package.PackagingError):
                package.parse_epoch(text)

    def test_native_host_gate(self):
        for target, (system, machine) in package.TARGETS.items():
            package.require_native_target(target, system, machine)
            for other_system, other_machine in (("Windows", machine), (system, "wrong")):
                with self.assertRaises(package.PackagingError):
                    package.require_native_target(target, other_system, other_machine)
        for target in ("../x", "aarch64-unknown-linux-gnu", "universal-apple-darwin"):
            with self.assertRaises(package.PackagingError):
                package.require_native_target(target, "Linux", "x86_64")


class HeaderTests(unittest.TestCase):
    def inspect(self, data, target):
        package.inspect_header(io.BytesIO(data), len(data), target)

    def test_thin_native_layouts(self):
        self.inspect(elf_data(), LINUX)
        self.inspect(macho_data(), INTEL_MAC)
        self.inspect(macho_data(cpu=0x0100000C), ARM_MAC)
        # A legitimate section table is optional (stripped ELF files need none).
        self.inspect(elf_data(shoff=120, shsize=64, shnum=1) + bytes(64), LINUX)

    def test_wrong_platform_or_architecture(self):
        for data, target in (
            (elf_data(machine=183), LINUX), (elf_data(), INTEL_MAC),
            (macho_data(), LINUX), (macho_data(), ARM_MAC),
            (macho_data(cpu=0x0100000C), INTEL_MAC), (macho_data(cpu=7), INTEL_MAC),
            (struct.pack(">II", 0xCAFEBABE, 2) + bytes(120), INTEL_MAC),
            (struct.pack(">II", 0xCAFEBABF, 2) + bytes(120), ARM_MAC),
        ):
            with self.subTest(target=target, magic=data[:4]), self.assertRaises(package.PackagingError):
                self.inspect(data, target)

    def test_elf_truncation_endianness_and_layout(self):
        for data in (
            b"", b"not an executable", elf_data()[:63], elf_data()[:119],
            elf_data(ident=b"\x7fELF\x01\x01\x01" + bytes(9)),
            elf_data(ident=b"\x7fELF\x02\x02\x01" + bytes(9)),
            elf_data(ident=b"\x7fELF\x02\x01\x00" + bytes(9)),
            elf_data(kind=1), elf_data(version=0), elf_data(ehsize=52),
            elf_data(phoff=63), elf_data(phoff=121), elf_data(phsize=32),
            elf_data(phnum=0), elf_data(phnum=0xFFFF),
            elf_data(shoff=120, shnum=1, shsize=64), elf_data(shstr=1),
            elf_data(shoff=64), elf_data(shoff=64, shnum=1, shsize=32),
            elf_data(shoff=120, shnum=1, shsize=64, shstr=1) + bytes(64),
        ):
            with self.subTest(header=data[:64]), self.assertRaises(package.PackagingError):
                self.inspect(data, LINUX)

    def test_macho_truncation_endianness_and_layout(self):
        for data in (
            b"", macho_data()[:31], macho_data()[:39],
            struct.pack(">IiiIIIII", 0xFEEDFACF, 0x01000007, 3, 2, 1, 24, 0, 0) + bytes(24),
            macho_data(kind=1), macho_data(reserved=1), macho_data(ncmds=0),
            macho_data(ncmds=2), macho_data(cmdbytes=7), macho_data(cmdbytes=16),
            macho_data()[:32] + struct.pack("<II", 0x1B, 0) + bytes(16),
            macho_data()[:32] + struct.pack("<II", 0x1B, 9) + bytes(16),
            macho_data()[:32] + struct.pack("<II", 0x1B, 32) + bytes(16),
            macho_data(cmdbytes=32) + bytes(8),
        ):
            with self.subTest(header=data[:32]), self.assertRaises(package.PackagingError):
                self.inspect(data, INTEL_MAC)


class ContentTests(unittest.TestCase):
    def test_desktop_has_no_open_file_codes_or_new_icon(self):
        fields = dict(line.split("=", 1) for line in package.DESKTOP.decode().splitlines()[1:])
        self.assertEqual(fields, {
            "Type": "Application", "Name": "Drift GUI", "Exec": "drift-gui",
            "TryExec": "drift-gui", "Terminal": "false", "Categories": "Development;Network;",
        })
        self.assertNotIn(b"%", package.DESKTOP)

    def test_mac_plist_exact_schema(self):
        for target in (INTEL_MAC, ARM_MAC):
            contents = package.archive_contents(VERSION, target, b"MIT test data", "")
            data = contents["Drift GUI.app/Contents/Info.plist"][1]
            self.assertTrue(data.startswith(b"<?xml"))
            self.assertEqual(plistlib.loads(data), {
                "CFBundleIdentifier": "io.github.WariKoda.drift-gui",
                "CFBundleName": "Drift GUI", "CFBundleDisplayName": "Drift GUI",
                "CFBundleExecutable": "drift-gui", "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": "1.2.3", "CFBundleVersion": "1.2.3",
                "DriftBuildVersion": VERSION, "LSMinimumSystemVersion": "15.0",
                "NSHighResolutionCapable": True, "NSPrincipalClass": "NSApplication",
            })
            readme = contents["README.md"][1].decode()
            for warning in ("TEST ONLY", "SSH dependency", "Monokai", "native GUI", "No Developer ID", "notarization", "Gatekeeper", "ad-hoc", "Native macOS CI", "not a universal"):
                self.assertIn(warning, readme)

    def test_linux_runtime_warnings(self):
        contents = package.archive_contents(VERSION, LINUX, b"MIT test data", "inventory marker")
        readme = contents["README.md"][1].decode()
        for warning in ("TEST ONLY", "SSH dependency", "Monokai", "native GUI", "XDG", "No installer"):
            self.assertIn(warning, readme)
        runtime = contents["RUNTIME.md"][1].decode()
        for warning in ("Ubuntu 24.04 x86_64", "not bundled", "ELF interpreter", "readelf", "No Linux portability", "inventory marker"):
            self.assertIn(warning, runtime)

    def test_environment_is_isolated(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            env = package.isolated_environment(home)
            self.assertEqual(env["HOME"], str(home))
            for name in ("DISPLAY", "WAYLAND_DISPLAY", "DRIFT_LOG", "DRIFT_DEBUG", "LD_PRELOAD", "DYLD_INSERT_LIBRARIES"):
                self.assertNotIn(name, env)
            for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
                path = Path(env[name])
                self.assertEqual(path.parent, home)
                self.assertTrue(path.is_dir())
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o700)


class ProcessTests(unittest.TestCase):
    def test_real_native_process_success_and_nonzero_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            env = package.isolated_environment(home)
            self.assertEqual(package.bounded_command([sys.executable, "-c", "print('native process')"], env, home), b"native process\n")
            with self.assertRaisesRegex(package.PackagingError, "command failed"):
                package.bounded_command([sys.executable, "-c", "raise SystemExit(7)"], env, home)

    def test_real_native_process_output_is_bounded_and_not_echoed(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            env = package.isolated_environment(home)
            for stream in (1, 2):
                with self.subTest(stream=stream), self.assertRaisesRegex(package.PackagingError, "output limit"):
                    package.bounded_command([sys.executable, "-c", f"import os; os.write({stream}, b'x' * 65537)"], env, home)


class ArchiveTests(unittest.TestCase):
    def test_contents_permissions_and_checksum_all_targets(self):
        license_data = (Path(__file__).resolve().parents[2] / "LICENSE").read_bytes()
        self.assertTrue(license_data.startswith(b"MIT License\n"))
        for target in package.TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                binary = root / "structural-data-only"
                binary_data = elf_data() if target == LINUX else macho_data(
                    cpu=0x01000007 if target == INTEL_MAC else 0x0100000C,
                )
                binary.write_bytes(binary_data)
                # These data fixtures are never executed or passed through the CLI.
                self.assertFalse(binary.stat().st_mode & 0o111)
                epoch = 1700000000
                with tempfile.TemporaryDirectory(prefix=".stage-", dir=root) as staging:
                    staged = Path(staging) / "archive.tar.gz.partial"
                    package.write_archive(staged, binary, VERSION, target, epoch, license_data, "structural inventory")
                    basename = f"drift-gui-{VERSION}-{target}.tar.gz"
                    final = package.publish_archive(staged, root, basename)
                self.assertEqual(sorted(path.name for path in root.iterdir()), [basename, basename + ".sha256", binary.name])
                self.assertEqual(stat.S_IMODE(final.stat().st_mode), 0o644)
                checksum = root / (basename + ".sha256")
                self.assertEqual(stat.S_IMODE(checksum.stat().st_mode), 0o644)
                self.assertEqual(checksum.read_text(), f"{hashlib.sha256(final.read_bytes()).hexdigest()}  {basename}\n")
                compressed = final.read_bytes()
                self.assertEqual(compressed[3], 0)  # No gzip filename or optional fields.
                self.assertEqual(struct.unpack("<I", compressed[4:8])[0], epoch)
                top = basename[:-7]
                contents = package.archive_contents(VERSION, target, license_data, "structural inventory")
                expected_names = {top}
                for relative in contents:
                    path = Path(top, relative)
                    expected_names.add(path.as_posix())
                    expected_names.update(parent.as_posix() for parent in path.parents if str(parent) != ".")
                with tarfile.open(final, "r:gz") as archive:
                    self.assertEqual(archive.getnames(), sorted(expected_names))
                    for member in archive.getmembers():
                        self.assertEqual((member.uid, member.gid, member.uname, member.gname, member.mtime), (0, 0, "", "", epoch))
                        self.assertFalse(member.issym() or member.islnk())
                        if member.isdir():
                            self.assertEqual(member.mode, 0o755)
                        else:
                            relative = member.name[len(top) + 1:]
                            mode, data = contents[relative]
                            self.assertEqual(member.mode, mode)
                            with archive.extractfile(member) as stream:
                                self.assertEqual(stream.read(), binary_data if data is None else data)

    def test_repeatable_across_staging_names_and_source_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = root / "first-data"
            second = root / "other-data"
            first.write_bytes(elf_data())
            second.write_bytes(elf_data())
            second.chmod(0o600)
            os.utime(second, (123, 123))
            for target in package.TARGETS:
                # Includes long PAX paths as well as regular tar entries.
                for version in (VERSION, "1.2.3-" + "a" * 122):
                    with tempfile.TemporaryDirectory(dir=root) as stage_a, tempfile.TemporaryDirectory(dir=root) as stage_b:
                        one = Path(stage_a) / "one.tar.gz.partial"
                        two = Path(stage_b) / "two.tar.gz.partial"
                        package.write_archive(one, first, version, target, 0, b"MIT data", "same inventory")
                        package.write_archive(two, second, version, target, 0, b"MIT data", "same inventory")
                        self.assertEqual(one.read_bytes(), two.read_bytes())
                        self.assertEqual(gzip.decompress(one.read_bytes()), gzip.decompress(two.read_bytes()))

    def test_refuse_existing_artifacts_including_dangling_symlink(self):
        for kind in ("archive", "checksum", "archive-directory", "checksum-symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                staging = root / "stage"
                staging.mkdir()
                staged = staging / "archive.tar.gz.partial"
                staged.write_bytes(b"structural archive data")
                basename = f"drift-gui-{VERSION}-{LINUX}.tar.gz"
                archive, checksum = root / basename, root / (basename + ".sha256")
                occupied = checksum if kind.startswith("checksum") else archive
                if kind.endswith("symlink"):
                    occupied.symlink_to("missing")
                elif kind.endswith("directory"):
                    occupied.mkdir()
                else:
                    occupied.write_bytes(b"must not be overwritten")
                with self.assertRaises(package.PackagingError):
                    package.publish_archive(staged, root, basename)
                self.assertTrue(os.path.lexists(occupied))
                self.assertFalse(os.path.lexists(archive if occupied == checksum else checksum))
                self.assertEqual(list(staging.iterdir()), [staged])
                if occupied.is_file():
                    self.assertEqual(occupied.read_bytes(), b"must not be overwritten")

    def test_invalid_metadata_never_creates_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "data"
            binary.write_bytes(b"data")
            for version, target, epoch in (("../escape", LINUX, 0), (VERSION, "../escape", 0), (VERSION, LINUX, -1)):
                archive = root / "never.tar.gz.partial"
                with self.assertRaises(package.PackagingError):
                    package.write_archive(archive, binary, version, target, epoch, b"MIT", "")
                self.assertFalse(archive.exists())


class CliRejectionTests(unittest.TestCase):
    def setUp(self):
        self.target = next((target for target, host in package.TARGETS.items() if host == (platform.system(), platform.machine())), None)
        if self.target is None:
            self.skipTest("no supported native host")

    def test_invalid_input_cleanup_without_executing_fixtures(self):
        for kind in ("missing", "empty", "symlink", "directory", "fifo", "invalid-header", "wrong-platform"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                binary = root / "binary"
                if kind == "symlink":
                    (root / "data").write_bytes(b"data")
                    binary.symlink_to("data")
                elif kind == "directory":
                    binary.mkdir()
                elif kind == "fifo":
                    os.mkfifo(binary)
                elif kind == "wrong-platform":
                    binary.write_bytes(macho_data() if self.target == LINUX else elf_data())
                elif kind != "missing":
                    binary.write_bytes(b"" if kind == "empty" else b"not an executable")
                output = root / "output"
                with self.assertRaises((package.PackagingError, OSError)):
                    package.package_binary(binary, VERSION, self.target, output, 0)
                self.assertEqual(list(output.iterdir()), [])

    def test_host_mismatch_rejected_before_open_or_execute(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = next(target for target in package.TARGETS if target != self.target)
            with self.assertRaisesRegex(package.PackagingError, "native host"):
                package.package_binary(root / "missing", VERSION, target, root / "output", 0)
            self.assertFalse((root / "output").exists())

    def test_cli_reports_generic_diagnostic(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "private-input"
            binary.write_bytes(b"sensitive binary contents\x1b[31m")
            output = root / "output"
            errors = io.StringIO()
            with contextlib.redirect_stderr(errors), self.assertRaises(SystemExit) as exit_info:
                package.main(["--binary", str(binary), "--version", VERSION, "--target", self.target, "--output", str(output)])
            self.assertEqual(exit_info.exception.code, 1)
            self.assertNotIn("sensitive", errors.getvalue())
            self.assertNotIn("private-input", errors.getvalue())
            self.assertNotIn("\x1b", errors.getvalue())
            self.assertEqual(list(output.iterdir()), [])

    def test_cli_has_no_validation_bypass(self):
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors), self.assertRaises(SystemExit) as exit_info:
            package.main(["--binary", "missing", "--version", VERSION, "--target", self.target, "--output", "unused", "--skip-validation"])
        self.assertEqual(exit_info.exception.code, 2)

    def test_cli_refuses_existing_checksum_before_opening_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            checksum = output / f"drift-gui-{VERSION}-{self.target}.tar.gz.sha256"
            checksum.write_bytes(b"existing")
            with self.assertRaisesRegex(package.PackagingError, "refusing overwrite"):
                package.package_binary(output / "missing", VERSION, self.target, output, 0)
            self.assertEqual(checksum.read_bytes(), b"existing")
            self.assertEqual(list(output.iterdir()), [checksum])


if __name__ == "__main__":
    unittest.main()
