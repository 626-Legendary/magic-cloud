#!/usr/bin/env python3
"""Install storage recovery and optional UUID mounting without initializing data."""
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
TARGET = '/mnt/magic-cloud'


def configuration(text):
    modes = re.findall(r'^STORAGE_MODE=(.*)$', text, re.MULTILINE)
    if len(modes) > 1 or (modes and modes[0] not in ('local', 'external')):
        raise ValueError('Invalid STORAGE_MODE')
    result = {'STORAGE_MODE': modes[0] if modes else 'external'}
    keys = ['STORAGE_MOUNT', 'STORAGE_ID']
    if result['STORAGE_MODE'] == 'external':
        keys.append('STORAGE_UUID')
    for key in keys:
        values = re.findall(rf'^{key}=(.*)$', text, re.MULTILINE)
        if len(values) != 1 or not values[0]:
            raise ValueError(f'Missing or duplicate configuration: {key}')
        result[key] = values[0]
    if result['STORAGE_MODE'] == 'external' and not re.fullmatch(r'[a-fA-F0-9-]+', result['STORAGE_UUID']):
        raise ValueError('Invalid STORAGE_UUID')
    return result


def fixed_config(text):
    if configuration(text)['STORAGE_MODE'] == 'local':
        return text
    return re.sub(r'^STORAGE_MOUNT=.*$', f'STORAGE_MOUNT={TARGET}', text, flags=re.MULTILINE)


def fstab_config(text, uuid):
    entry = f'UUID={uuid} {TARGET} ext4 defaults,nofail,nodev,nosuid,noexec,x-systemd.device-timeout=10s 0 2'
    found = False
    lines = text.splitlines()
    for index, line in enumerate(lines):
        fields = line.split()
        if not fields or line.lstrip().startswith('#'):
            continue
        if fields[0] == f'UUID={uuid}' or (len(fields) > 1 and fields[1] == TARGET):
            if len(fields) < 3 or fields[:3] != [f'UUID={uuid}', TARGET, 'ext4'] or found:
                raise ValueError('Conflicting /etc/fstab entry; resolve it before installation')
            lines[index] = entry
            found = True
    if not found:
        lines.extend(['# Magic Cloud data disk (USB drive, HDD or SSD)', entry])
    return '\n'.join(lines) + '\n'


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def main():
    if os.geteuid() != 0:
        raise ValueError('Administrator access required: sudo python3 deploy/scripts/install-storage.py')
    if re.search(r'[\s%\\"\x00-\x1f]', str(PROJECT)):
        raise ValueError('Project path contains characters unsupported by the systemd template')
    for command in ('docker', 'systemctl', 'findmnt', 'mount'):
        if not shutil.which(command):
            raise ValueError(f'Required command not found: {command}')
    env_path = PROJECT / '.env'
    old_config = env_path.read_text()
    config = configuration(old_config)
    new_config = fixed_config(old_config)
    fstab = Path('/etc/fstab')
    old_fstab = fstab.read_text()
    new_fstab = old_fstab
    candidates = [config['STORAGE_MOUNT']]
    if config['STORAGE_MODE'] == 'external':
        new_fstab = fstab_config(old_fstab, config['STORAGE_UUID'])
        target = Path(TARGET)
        if target.resolve() != target:
            raise ValueError('Fixed mount path must not contain symlinks')
        mounts = json.loads(subprocess.check_output(
            ['findmnt', '--json', '--list', '-o', 'TARGET,UUID,FSTYPE'], text=True))['filesystems']
        for mount in mounts:
            if mount['target'] == TARGET and mount.get('uuid') != config['STORAGE_UUID']:
                raise ValueError('Fixed mount path is occupied by another filesystem')
        if not os.path.ismount(TARGET) and target.exists() and any(target.iterdir()):
            raise ValueError('Unmounted fixed mount directory is not empty; refusing to hide its contents')
        candidates = [m['target'] for m in mounts if m.get('uuid') == config['STORAGE_UUID']]
        if not candidates:
            raise ValueError('Expected data disk is not mounted; connect and mount it before installation')
    # Validate the existing disk, layout, marker and writable state before changing the host.
    script = str(PROJECT / 'deploy/scripts/storage.sh')
    with tempfile.TemporaryDirectory(prefix='magic-cloud-install-') as temporary:
        temporary_config = Path(temporary) / '.env'
        temporary_config.write_text(re.sub(
            r'^STORAGE_MOUNT=.*$', lambda _: f'STORAGE_MOUNT={candidates[0]}',
            old_config, flags=re.MULTILINE))
        temporary_config.chmod(0o600)
        env = dict(os.environ, MC_CONFIG=str(temporary_config))
        run('bash', script, 'check', env=env)
        temporary_config.write_text(new_config)
        run('bash', script, 'mount', env=env)
        run('bash', script, 'check', env=env)

    compose = ['docker', 'compose', '--project-directory', str(PROJECT), '--env-file', str(env_path)]
    # Keep an existing desktop mount intact; containers will use the fixed mount.
    units = Path('/etc/systemd/system')
    if (units / 'magic-cloud-storage.timer').exists():
        run('systemctl', 'stop', 'magic-cloud-storage.timer', 'magic-cloud-storage.service')
    run(*compose, 'stop', '-t', '15', 'magic-cloud')
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    if new_fstab != old_fstab:
        backup = fstab.with_name(f'fstab.magic-cloud-{stamp}.bak')
        shutil.copy2(fstab, backup)
        fstab.write_text(new_fstab)
        print(f'fstab backup: {backup}', flush=True)
    env_path.write_text(new_config)
    for name in ('magic-cloud-storage.service', 'magic-cloud-storage.timer'):
        content = (PROJECT / 'deploy/systemd' / name).read_text().replace('/opt/magic-cloud', str(PROJECT))
        (units / name).write_text(content)
        (units / name).chmod(0o644)
    run('systemctl', 'daemon-reload')
    # Recreate to replace the old container's bind mount.
    run(*compose, 'up', '-d', '--no-deps', '--force-recreate', 'magic-cloud')
    run('systemctl', 'enable', '--now', 'magic-cloud-storage.timer')
    location = TARGET if config['STORAGE_MODE'] == 'external' else config['STORAGE_MOUNT']
    print(f'Storage configured: {config["STORAGE_MODE"]} -> {location}', flush=True)
    print('No data was initialized or formatted. Verify: docker compose ps', flush=True)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f'Storage installation failed: {error}')
