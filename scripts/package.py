"""Build-time archive assembly; never used by the Rust application."""
import gzip
import argparse
import hashlib
import json
import os
import re
import tarfile
import tempfile
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import zipfile


# winit/wgpu load these at runtime, outside ELF NEEDED. Keep both supported
# desktop backends installed; dpkg-shlibdeps supplies actual linked ABI minima.
LINUX_RUNTIME_DEPENDS = (
    'libudev1', 'libxkbcommon0', 'libxkbcommon-x11-0',
    'libwayland-client0', 'libwayland-cursor0', 'libwayland-egl1',
    'libx11-6', 'libx11-xcb1', 'libxcursor1', 'libxi6', 'libxrandr2',
    'libegl1', 'libgles2', 'libvulkan1', 'libegl-mesa0', 'libgl1-mesa-dri',
)
LINUX_GLIBC_CEILING = (2, 35)
DESKTOP_ENTRY = """[Desktop Entry]
Type=Application
Name=Hejjetrac
GenericName=Spherical motion tracker
Comment=Track spherical treadmills, calibrate cameras and record experiments
Exec=hejjetrac
TryExec=hejjetrac
Icon=hejjetrac
Terminal=false
Categories=Science;Education;
Keywords=Hejjetrac;tracking;camera;laboratory;
StartupNotify=false
StartupWMClass=hejjetrac
Actions=SafeMode;

[Desktop Action SafeMode]
Name=Software graphics mode
Exec=hejjetrac --safe-mode
"""


def workspace_version(manifest, *, debian=False):
    text = manifest.read_text()
    match = re.search(r'(?ms)^\[workspace\.package\]\s*\n(.*?)(?=^\[|\Z)', text)
    version = re.search(r'(?m)^version\s*=\s*"([^"\n]+)"', match.group(1)) if match else None
    if not version or not re.fullmatch(r'[0-9][0-9A-Za-z.+~-]*', version.group(1)):
        raise ValueError('Cannot determine a Debian-compatible workspace version')
    return version.group(1).replace('-', '~') if debian else version.group(1)


def verify_release_tag(manifest, tag):
    version = workspace_version(manifest)
    if tag != 'v' + version:
        raise ValueError(f'Release {tag!r} does not match source version v{version}; '
                         'check out the matching source before publishing binaries')


def verify_binary_version(binary, version):
    result = subprocess.run([str(binary.resolve()), '--version'], check=True,
                            text=True, capture_output=True)
    actual = result.stdout.strip()
    if not actual or actual.split()[-1] != version:
        raise ValueError(f'{binary} reports {actual!r}, expected version {version}; '
                         'run cargo build --workspace --release --locked before packaging')


def install_binaries(root, destination, suffix, system, version):
    manifest = []
    for source, target in [('hejjetrac', 'hejjetrac'), ('hejjetrac-cli', 'hejjetrac-cli')]:
        compiled = root / 'target/release' / (source + suffix)
        verify_binary_version(compiled, version)
        installed = destination / (target + suffix)
        shutil.copy2(compiled, installed)
        manifest.append({'name': installed.name,
                         'compiled_sha256': hashlib.sha256(compiled.read_bytes()).hexdigest()})
    return manifest


def install_default_configs(root, resource):
    """Release defaults never come from a developer's calibrated rig files."""
    for rig in range(1, 5):
        source = root / 'examples/defaults' / f'VR{rig}.txt'
        text = source.read_text(encoding='utf-8')
        fields = dict(line.split('#', 1)[0].split(':', 1) for line in text.splitlines()
                      if ':' in line.split('#', 1)[0])
        fields = {key.strip(): value.strip() for key, value in fields.items()}
        if fields.get('rust_camera_id') or fields.get('src_fn') != str(rig - 1):
            raise ValueError(f'{source}: release default must be unbound with a portable ordinal')
        shutil.copy2(source, resource / source.name)


def write_zip_tree(stage, archive):
    # Preserve resource symlinks instead of duplicating their payloads.
    with zipfile.ZipFile(archive, 'x', compression=zipfile.ZIP_DEFLATED,
                         strict_timestamps=False) as output:
        for path in sorted(stage.rglob('*')):
            relative = str(path.relative_to(stage.parent))
            if path.is_symlink():
                entry = zipfile.ZipInfo(relative)
                entry.create_system = 3
                entry.external_attr = 0o120777 << 16
                output.writestr(entry, os.readlink(path))
            else:
                output.write(path, relative)


def linux_dependencies(stage):
    if platform.machine().lower() not in ('x86_64', 'amd64'):
        raise ValueError('Linux release packaging currently supports x86_64/amd64 only')
    with tempfile.TemporaryDirectory(prefix='hejjetrac-shlibdeps-') as temporary:
        directory = Path(temporary)
        (directory / 'debian').mkdir()
        (directory / 'debian/control').write_text(
            'Source: hejjetrac\nSection: science\nPriority: optional\n'
            'Maintainer: hejjetrac maintainers <noreply@github.com>\n\n'
            'Package: hejjetrac\nArchitecture: amd64\nDescription: Native spherical motion tracker\n')
        result = subprocess.run(['dpkg-shlibdeps', '-O',
                                 '-e' + str((stage / 'hejjetrac').resolve()),
                                 '-e' + str((stage / 'hejjetrac-cli').resolve())],
                                cwd=directory, check=True, text=True, capture_output=True)
    lines = [line.split('=', 1)[1] for line in result.stdout.splitlines()
             if line.startswith('shlibs:Depends=')]
    if len(lines) != 1 or not lines[0].strip():
        raise ValueError('dpkg-shlibdeps returned no linked runtime dependency evidence')
    # Fail if build-host metadata cannot account for an explicitly loaded library.
    installed = subprocess.check_output(
        ['dpkg-query', '--show', '--showformat=${Package} ${Status}\n',
         *LINUX_RUNTIME_DEPENDS], text=True).splitlines()
    if len(installed) != len(LINUX_RUNTIME_DEPENDS) or any(
            not line.endswith(' install ok installed') for line in installed):
        raise ValueError('Install all declared Linux runtime libraries before packaging')
    return lines[0] + ', ' + ', '.join(LINUX_RUNTIME_DEPENDS)


def linux_abi_requirements(stage):
    """Reject a newer build ABI instead of disguising it with weaker Depends."""
    evidence = []
    for name in ('hejjetrac', 'hejjetrac-cli'):
        binary = stage / name
        result = subprocess.run(['readelf', '--version-info', '--wide', str(binary)],
                                check=True, text=True, capture_output=True)
        # Version needs also expose requirements such as GLIBC_ABI_DT_RELR,
        # which are not ordinary undefined symbols and require newer loaders.
        names = set(re.findall(r'\bGLIBC_[A-Za-z0-9_.]+\b', result.stdout))
        numeric = [re.fullmatch(r'GLIBC_([0-9]+(?:\.[0-9]+)+)', name) for name in names]
        if not names or any(value is None for value in numeric):
            raise ValueError(f'{binary}: cannot establish a public glibc ABI from ELF symbols')
        versions = sorted({tuple(map(int, value.group(1).split('.'))) for value in numeric})
        if versions[-1] > LINUX_GLIBC_CEILING:
            required = '.'.join(map(str, versions[-1]))
            raise ValueError(f'{binary} requires glibc {required}; Ubuntu 22.04 supports 2.35. '
                             'Rebuild in the Ubuntu 22.04 release container; do not lower '
                             'the Debian dependency without rebuilding the executable')
        evidence.append({'binary': name,
                         'required_glibc_versions': ['.'.join(map(str, v)) for v in versions],
                         'maximum_required_glibc': '.'.join(map(str, versions[-1]))})
    return {'target_baseline': 'Ubuntu 22.04 / glibc 2.35',
            'glibc_ceiling': '.'.join(map(str, LINUX_GLIBC_CEILING)),
            'binaries': evidence}


def tar_gz_tree(directory, destination, epoch, executables=()):
    # dpkg accepts GNU long-name headers, but rejects PAX x/g extensions.
    # Full dependency notices can exceed the ordinary 100-byte name field.
    # Fixed root ownership/modes need no root or maintainer scripts.
    with destination.open('xb') as raw:
        with gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.GNU_FORMAT) as archive:
                for path in sorted(directory.rglob('*')):
                    info = archive.gettarinfo(str(path), str(path.relative_to(directory)))
                    info.uid = info.gid = 0
                    info.uname = info.gname = 'root'
                    info.mtime = epoch
                    if info.issym():
                        info.linkname = info.linkname.replace(os.sep, '/')
                    if info.isdir() or info.issym():
                        info.mode = 0o755 if info.isdir() else 0o777
                    else:
                        executable = (path.relative_to(directory).as_posix() in executables
                                      or path.stat().st_mode & 0o111)
                        info.mode = 0o755 if executable else 0o644
                    if info.isfile():
                        with path.open('rb') as source:
                            archive.addfile(info, source)
                    else:
                        archive.addfile(info)


def ar_member(output, name, source, epoch):
    size = source.stat().st_size
    header = (f'{name + "/":<16}{epoch:<12}{0:<6}{0:<6}{"100644":<8}{size:<10}`\n').encode('ascii')
    if len(header) != 60:
        raise ValueError('Debian archive member metadata exceeds ar format limits')
    output.write(header)
    with source.open('rb') as data:
        shutil.copyfileobj(data, output)
    if size % 2:
        output.write(b'\n')


def build_deb(stage, destination, version, dependencies, assets):
    if not dependencies.strip() or '\n' in dependencies or '\n' in version:
        raise ValueError('Debian version/dependencies must be nonempty single lines')
    epoch = int(os.environ.get('SOURCE_DATE_EPOCH', '0'))
    if not 0 <= epoch <= 0xffffffff:
        raise ValueError('SOURCE_DATE_EPOCH must fit an unsigned 32-bit gzip timestamp')
    destination = Path(destination)
    partial = destination.with_name(destination.name + '.partial')
    if destination.exists() or partial.exists():
        raise FileExistsError(f'Refusing to overwrite package: {destination}')
    with tempfile.TemporaryDirectory(prefix='hejjetrac-deb-') as temporary:
        temporary = Path(temporary)
        data = temporary / 'data'
        payload = data / 'usr/lib/hejjetrac'
        payload.parent.mkdir(parents=True)
        shutil.copytree(stage, payload, symlinks=True)
        for name in ('hejjetrac', 'hejjetrac-cli'):
            (payload / name).chmod(0o755)
            link = data / 'usr/bin' / name
            link.parent.mkdir(parents=True, exist_ok=True)
            link.symlink_to('../lib/hejjetrac/' + name)
        entry = data / 'usr/share/applications/hejjetrac.desktop'
        entry.parent.mkdir(parents=True)
        entry.write_bytes(DESKTOP_ENTRY.encode('utf-8'))
        for relative, source in (
            ('scalable/apps/hejjetrac.svg', 'hejjetrac.svg'),
            ('1024x1024/apps/hejjetrac.png', 'hejjetrac.png'),
        ):
            icon = data / 'usr/share/icons/hicolor' / relative
            icon.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(assets / source, icon)
        metadata = data / 'usr/share/metainfo/io.github.pvnkmrksk.hejjetrac.metainfo.xml'
        metadata.parent.mkdir(parents=True)
        shutil.copy2(assets / 'io.github.pvnkmrksk.hejjetrac.metainfo.xml', metadata)
        doc = data / 'usr/share/doc/hejjetrac'
        doc.mkdir(parents=True)
        shutil.copy2(stage / 'LICENSE', doc / 'copyright')
        for filename in ['README.md', 'CITATION.cff', 'THIRD_PARTY.md']:
            if (stage / filename).is_file():
                shutil.copy2(stage / filename, doc / filename)
        pictures = doc / 'screenshots'
        pictures.mkdir()
        for filename in ['preview.png', 'calibration.png', 'rig-grid-v050-20261107.png']:
            source = assets.parent / 'docs/images' / filename
            if source.is_file():
                shutil.copy2(source, pictures / filename)
        control = temporary / 'control'
        control.mkdir()
        installed_size = sum(p.stat().st_size for p in data.rglob('*') if p.is_file() and not p.is_symlink())
        control_text = (
            f'Package: hejjetrac\nVersion: {version}\nArchitecture: amd64\n'
            'Section: science\nPriority: optional\n'
            'Maintainer: Pavan Kumar Kaushik <noreply@github.com>\n'
            f'Installed-Size: {(installed_size + 1023) // 1024}\nDepends: {dependencies}\n'
            'Recommends: xdg-desktop-portal, xdg-desktop-portal-gtk | xdg-desktop-portal-kde\n'
            'Homepage: https://github.com/pvnkmrksk/hejjetrac\n'
            'Description: Hejjetrac - spherical treadmill tracking and experiment recording\n'
            ' Measure patterned-ball rotation with a native desktop application, guided\n'
            ' camera calibration and independent multi-camera rig sessions. Record source\n'
            ' and diagnostic video, all-frame numerical data and synchronization metadata.\n'
            ' Supports UDP and ZeroMQ motion output and authenticated remote recording\n'
            ' control for experiments using a separate virtual-reality computer.\n'
            ' .\n'
            ' Includes the hejjetrac desktop and hejjetrac-cli command-line tools, an\n'
            ' illustrated user manual, sample video, unbound camera defaults and complete\n'
            ' attribution and dependency notices. No compiler or Python runtime is needed.\n'
            ' Camera calibration is required for your own hardware. Based on FicTrac.\n')
        # Debian metadata uses LF even when these build-tool tests run on Windows.
        (control / 'control').write_bytes(control_text.encode('utf-8'))
        (temporary / 'debian-binary').write_bytes(b'2.0\n')
        tar_gz_tree(control, temporary / 'control.tar.gz', epoch)
        # Windows chmod cannot express Unix executable bits. Preserve the
        # install contract explicitly when exercising Debian assembly there.
        executables = {f'usr/lib/hejjetrac/{name}' for name in
                       ('hejjetrac', 'hejjetrac-cli')}
        tar_gz_tree(data, temporary / 'data.tar.gz', epoch, executables)
        with partial.open('xb') as output:
            output.write(b'!<arch>\n')
            for name in ('debian-binary', 'control.tar.gz', 'data.tar.gz'):
                ar_member(output, name, temporary / name, epoch)
        partial.rename(destination)
    return destination


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check-release-tag', help='Check source/tag identity without packaging')
    arguments = parser.parse_args()
    if arguments.check_release_tag:
        verify_release_tag(root / 'Cargo.toml', arguments.check_release_tag)
        print(f'Source version matches {arguments.check_release_tag}')
        return
    version = workspace_version(root / 'Cargo.toml')
    system = platform.system()
    arch = platform.machine().lower()
    label = {'Darwin': 'macos', 'Windows': 'windows', 'Linux': 'linux'}[system]
    name = f'hejjetrac-{label}-{arch}'
    stage = root / 'dist' / name
    stage.mkdir(parents=True, exist_ok=False)
    suffix = '.exe' if system == 'Windows' else ''
    if system == 'Darwin':
        app = stage / 'hejjetrac.app' / 'Contents'
        binary = app / 'MacOS'
        resource = app / 'Resources'
        binary.mkdir(parents=True)
        resource.mkdir()
        info = plistlib.loads((root / 'apps/hejjetrac-desktop/Info.plist').read_bytes())
        if any(info.get(key) != version for key in ('CFBundleVersion', 'CFBundleShortVersionString')):
            raise ValueError('Info.plist version does not match Cargo.toml; update both before packaging')
        info['CFBundleExecutable'] = 'hejjetrac'
        if (root / 'assets/hejjetrac.icns').is_file():
            info['CFBundleIconFile'] = 'hejjetrac.icns'
            shutil.copy2(root / 'assets/hejjetrac.icns', resource / 'hejjetrac.icns')
        (app / 'Info.plist').write_bytes(plistlib.dumps(info))
    else:
        binary = resource = stage
    binaries = install_binaries(root, binary, suffix, system, version)
    linux_abi = linux_depends = None
    if system == 'Linux':
        # Audit before either archive is created: the portable tar has the same
        # linked ABI as the Debian package and must obey the same baseline.
        linux_abi = linux_abi_requirements(stage)
        linux_depends = linux_dependencies(stage)
    for directory in ['examples', 'docs', 'book']:
        shutil.copytree(root / directory, resource / directory)
    install_default_configs(root, resource)
    for filename in ['README.md', 'LICENSE', 'ATTRIBUTION.md', 'THIRD_PARTY.md', 'CITATION.cff']:
        shutil.copy2(root / filename, resource / filename)
    licenses = resource / 'DependencyLicenses'
    licenses.mkdir()
    manifest = []
    lucide_notices = root / 'assets/lucide'
    if lucide_notices.is_dir():
        destination = licenses / 'lucide'
        destination.mkdir()
        for filename in ['LICENSE', 'README.md']:
            shutil.copy2(lucide_notices / filename, destination / filename)
        manifest.append({'package': 'lucide', 'notice_files': ['LICENSE', 'README.md']})
    packages = list((root / 'third-party-source').iterdir())
    if (root / 'vendor').is_dir():
        packages.extend((root / 'vendor').iterdir())
    for package in sorted(packages):
        if not package.is_dir():
            continue
        names = []
        for path in package.rglob('*'):
            if path.is_file() and (path.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright')) or path.name == 'HEJJETRAC_PATCHES.md'):
                relative = path.relative_to(package)
                destination = licenses / package.name / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, destination)
                names.append(str(relative))
        manifest.append({'package': package.name, 'notice_files': names})
    (licenses / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    build = {
        'name': 'HejjéTrac', 'version': version, 'platform': label,
        'architecture': arch, 'source_commit': os.environ.get('PACKAGE_SOURCE_COMMIT'),
        'source_url': 'https://github.com/pvnkmrksk/hejjetrac',
        'binary_hash_boundary': 'compiled binaries before application signing',
        'binaries': binaries,
    }
    if linux_abi is not None:
        build['linux_abi'] = linux_abi
        build['linux_depends'] = linux_depends
    (resource / 'BUILD.json').write_text(json.dumps(build, indent=2, ensure_ascii=False) + '\n')
    if system == 'Darwin':
        subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(stage / 'hejjetrac.app')], check=True)
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(stage / 'hejjetrac.app')], check=True)
    if system in ('Darwin', 'Windows'):
        archive = root / 'dist' / (name + '.zip')
        # Cargo registry source archives use reproducible pre-1980 mtimes. Keep their
        # notice content intact while clamping only ZIP's unrepresentable metadata.
        write_zip_tree(stage, archive)
    else:
        archive = shutil.make_archive(str(root / 'dist' / name), 'gztar', root_dir=stage.parent, base_dir=stage.name)
    print(archive)
    if system == 'Linux':
        version = workspace_version(root / 'Cargo.toml', debian=True)
        print(build_deb(stage, root / 'dist' / 'hejjetrac-linux-amd64.deb', version, linux_depends, root / 'assets'))


if __name__ == "__main__":
    main()
