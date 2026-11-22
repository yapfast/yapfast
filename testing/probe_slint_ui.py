#!/usr/bin/env python3
"""Exercise Yapfast on the private compositor and capture states for visual review."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import signal
import subprocess
import tempfile
import time
import urllib.parse
import uuid

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--renderer', choices=['gpu', 'software'], default='gpu')
parser.add_argument('--size', choices=['1120x760', '800x560'], default='1120x760')
parser.add_argument('--connected', action='store_true', help='Create a disposable local Matrix account and check encrypted UI sending/restoration.')
parser.add_argument('--long-name', action='store_true', help='Use a long channel label in the connected fixture.')
parser.add_argument('--exe', type=Path, default=ROOT / 'target/fast/yapfast')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
width, height = map(int, args.size.split('x'))
environment = dict(os.environ, YAPFAST_UI_TEST_SIZE=args.size)
session_directory = args.output / ('connected-session-' + uuid.uuid4().hex if args.connected else 'unused-preview-session')
command = [str(args.exe.resolve()), '--renderer', args.renderer,
           '--data-dir', str(session_directory)]
if not args.connected:
    command.insert(1, '--preview')
seed = None
wire_check = {}
if args.connected:
    helpers = runpy.run_path(str(ROOT / 'testing/check_stack.py'))
    request, require = helpers['request'], helpers['require']
    homeserver = 'http://localhost:18008'
    username = 'slint' + uuid.uuid4().hex
    password = 'talktestonly'
    require(*request(homeserver, 'POST', '/_matrix/client/v3/register', {
        'username': username, 'password': password, 'inhibit_login': True,
        'auth': {'type': 'm.login.dummy'},
    }))
    seed = require(*request(homeserver, 'POST', '/_matrix/client/v3/login', {
        'type': 'm.login.password', 'identifier': {'type': 'm.id.user', 'user': username},
        'password': password,
    }))
    encrypted = require(*request(homeserver, 'POST', '/_matrix/client/v3/createRoom', {
        'name': 'general discussion for our friends planning the next weekend together' if args.long_name else 'general',
        'preset': 'private_chat', 'initial_state': [{
            'type': 'm.room.encryption', 'state_key': '',
            'content': {'algorithm': 'm.megolm.v1.aes-sha2'},
        }],
    }, seed['access_token']))['room_id']
    space = require(*request(homeserver, 'POST', '/_matrix/client/v3/createRoom', {
        'name': 'Slint validation', 'creation_content': {'type': 'm.space'},
        'initial_state': [{
            'type': 'm.space.child', 'state_key': encrypted,
            'content': {'via': ['synapse']},
        }],
    }, seed['access_token']))['room_id']


def capture(name):
    time.sleep(.25)
    with tempfile.TemporaryDirectory(prefix='yapfast-slint-shot-') as folder:
        subprocess.run(['weston-screenshooter'],
                       env=dict(environment, XDG_PICTURES_DIR=folder),
                       check=True, timeout=10, capture_output=True)
        pictures = list(Path(folder).glob('*.png'))
        if len(pictures) != 1:
            raise RuntimeError('Private compositor did not produce a screenshot')
        path = args.output / f'{name}.png'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y',
                        '-i', str(pictures[0]), '-vf', 'vflip', str(path)],
                       check=True, timeout=10)
        return path


def locate_rail(picture):
    # Locate the rail in our private screenshot, rather than assume where
    # Weston placed the window. Exact neutral pixels form its long left edge.
    pixels = subprocess.check_output(['ffmpeg', '-hide_banner', '-loglevel', 'error',
                                     '-i', str(picture), '-f', 'rawvideo',
                                     '-pix_fmt', 'rgb24', '-'], timeout=10)
    output_width, output_height = 1600, 1000
    if len(pixels) != output_width * output_height * 3:
        raise RuntimeError('Unexpected private output dimensions')
    target = bytes([14, 14, 14])
    columns = [sum(pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target
                   for y in range(output_height)) for x in range(output_width)]
    best = max(columns)
    if best < height - 80:
        raise RuntimeError('Yapfast rail was not found; inspect startup.png and app.log')
    x = next(x for x, count in enumerate(columns) if count >= best - 2)
    ys = [y for y in range(output_height)
          if pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target]
    return x, min(ys)


with (args.output / 'app.log').open('wb') as log:
    process = subprocess.Popen(command, env=environment, stdout=log, stderr=log,
                               start_new_session=True)
    try:
        time.sleep(2)
        if process.poll() is not None:
            raise RuntimeError('Yapfast exited before the first capture; inspect app.log')
        origin_x, origin_y = locate_rail(capture('startup'))

        def click(x, y):
            INPUT['click'](origin_x + x, origin_y + y)
            time.sleep(.15)

        def type_text(text):
            keys = dict(zip('abcdefghijklmnopqrstuvwxyz',
                            [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50,
                             49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44]))
            keys.update(dict(zip('1234567890', range(2, 12))))
            keys.update({' ': 57, '.': 52, '/': 53, '-': 12})
            for character in text:
                if character == ':':
                    INPUT['send']('key 42 1')
                    INPUT['key'](39)
                    INPUT['send']('key 42 0')
                else:
                    INPUT['key'](keys[character])

        if args.connected:
            # The username field receives initial focus. All input goes to the
            # private seat; these disposable credentials never reach a log.
            type_text(username)
            INPUT['key'](15)
            type_text(password)
            INPUT['send']('key 42 1')
            INPUT['key'](15)
            INPUT['key'](15)
            INPUT['send']('key 42 0')
            INPUT['send']('key 29 1')
            INPUT['key'](30)
            INPUT['send']('key 29 0')
            type_text(homeserver)
            INPUT['key'](15)
            INPUT['key'](15)
            INPUT['key'](28)
            time.sleep(12)
            capture('signed-in')
            click(32, 101)
            click(150, 159)
            time.sleep(2)
            click(390, height - 65)
            type_text('encrypted slint test')
            INPUT['key'](28)
            time.sleep(3)
            capture('encrypted-sent')
            room_path = urllib.parse.quote(encrypted, safe='')
            events = require(*request(homeserver, 'GET',
                '/_matrix/client/v3/rooms/' + room_path + '/messages?dir=b&limit=20',
                token=seed['access_token']))['chunk']
            messages = [event for event in events if event['type'] in ('m.room.message', 'm.room.encrypted')]
            if len(messages) != 1 or messages[0]['type'] != 'm.room.encrypted':
                raise RuntimeError('Expected one encrypted message from the Slint UI; inspect captures')
            if 'encrypted slint test' in json.dumps(messages[0]['content']):
                raise RuntimeError('The encrypted wire event contains the plaintext fixture')
            wire_check = {'encrypted_wire_event': True, 'plaintext_absent_from_wire': True}
            click(width - 18, -18)  # Close on the private window's title bar.
            if process.wait(timeout=10) != 0:
                raise RuntimeError('Normal window close failed')
            wire_check['normal_close_exit_zero'] = True
            process = subprocess.Popen(command, env=environment, stdout=log, stderr=log,
                                       start_new_session=True)
            time.sleep(8)
            origin_x, origin_y = locate_rail(capture('restored'))
            click(32, 101)
            click(150, 159)
            time.sleep(2)
            capture('restored-timeline')
            click(150, height - 29)
            time.sleep(2)
            capture('signed-out')
            if (session_directory / 'active.json').exists():
                raise RuntimeError('Sign out did not clear active session metadata')
            wire_check['signed_out_session_cleared'] = True
        else:

            click(390, height - 65)
            type_text('slint private test')
            capture('typed')
            INPUT['key'](28)
            capture('sent')
            click(150, 197)
            type_text('remember this draft')
            capture('channel-draft')
            click(32, 36)
            capture('direct-messages')
            click(32, 101)
            capture('restored-server-draft')
            click(150, 303)
            capture('voice-empty')
            click(395, 208 if width == 1120 else 226)
            capture('voice-alone')
            click(150, 159)
            capture('voice-while-chatting')
            click(134, height - 137)
            capture('deafened')
            click(174, height - 137)
            capture('audio-devices')
            INPUT['key'](1)  # Escape dismisses the device popover.
            click(150, height - 29)
            capture('sign-in')
            # Password entry exercises native masking without connecting an account.
            INPUT['key'](15)
            type_text('masked password')
            capture('password-entry')
        (args.output / 'actions.json').write_text(json.dumps({
            'renderer': args.renderer, 'size': args.size, 'connected': args.connected,
            'long_name': args.long_name,
            'checks': wire_check,
            'origin': [origin_x, origin_y],
            'binary_sha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
            'isolation': 'Private Weston, virtual input and private D-Bus only. No microphone or speakers opened.',
            'validation': 'Coordinate actions and screenshots require visual inspection; captures alone do not assert success.',
        }, indent=2) + '\n')
    finally:
        if seed:
            request(homeserver, 'POST', '/_matrix/client/v3/logout', {}, seed['access_token'])
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
