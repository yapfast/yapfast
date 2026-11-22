#!/usr/bin/env python3
"""Check real local services without logging authentication credentials."""

import json
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid

HOMESERVER = "http://localhost:18008"
AUTHORIZATION = "http://localhost:16080"
PASSWORD = "talk-test-only"


def request(base, method, path, body=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        try:
            result = json.load(error)
        except (ValueError, UnicodeError):
            result = {}
        return error.code, result


def require(status, body, expected=200):
    if status != expected:
        raise RuntimeError(f"Expected HTTP {expected}, got {status}, {body.get('errcode', 'service error')}")
    return body


def check():
    versions = require(*request(HOMESERVER, "GET", "/_matrix/client/versions"))
    features = versions.get("unstable_features", {})
    for feature in ("org.matrix.simplified_msc3575", "org.matrix.msc4354", "org.matrix.msc4140"):
        if not features.get(feature):
            raise RuntimeError("Test homeserver is missing " + feature)

    sessions = []
    try:
        for user in ("alice", "bob", "charlie"):
            status, body = request(HOMESERVER, "POST", "/_matrix/client/v3/register", {
                "username": user,
                "password": PASSWORD,
                "inhibit_login": True,
                "auth": {"type": "m.login.dummy"},
            })
            if status != 200 and body.get("errcode") != "M_USER_IN_USE":
                require(status, body)
            session = require(*request(HOMESERVER, "POST", "/_matrix/client/v3/login", {
                "type": "m.login.password",
                "identifier": {"type": "m.id.user", "user": user},
                "password": PASSWORD,
                "device_id": "YAPFAST_TEST_" + user.upper(),
            }))
            sessions.append(session)

        alice = sessions[0]
        room = require(*request(HOMESERVER, "POST", "/_matrix/client/v3/createRoom", {
            "name": "Yapfast backend check",
            "preset": "private_chat",
            "initial_state": [{
                "type": "m.room.encryption",
                "state_key": "",
                "content": {"algorithm": "m.megolm.v1.aes-sha2"},
            }],
        }, alice["access_token"]))
        user = urllib.parse.quote(alice["user_id"], safe="")
        openid = require(*request(HOMESERVER, "POST",
            "/_matrix/client/v3/user/" + user + "/openid/request_token", {}, alice["access_token"]))
        claims = {
            "room_id": room["room_id"],
            "slot_id": "m.call#ROOM",
            "openid_token": openid,
            "member": {
                "id": str(uuid.uuid4()),
                "claimed_user_id": alice["user_id"],
                "claimed_device_id": alice["device_id"],
            },
        }
        result = require(*request(AUTHORIZATION, "POST", "/get_token", claims))
        if not result.get("jwt") or result.get("url") != "ws://localhost:17880":
            raise RuntimeError("Authorization did not return the expected local LiveKit transport")
        claims["openid_token"] = {**openid, "access_token": "invalid-test-token"}
        status, _ = request(AUTHORIZATION, "POST", "/get_token", claims)
        if status not in (401, 403):
            raise RuntimeError(f"Invalid OpenID token was not rejected as expected, HTTP {status}")
        print("Backend check passed: three accounts, Sliding Sync, sticky events, encrypted room, SFU authorization and invalid-token rejection.")
    finally:
        for session in sessions:
            request(HOMESERVER, "POST", "/_matrix/client/v3/logout", {}, session["access_token"])


if __name__ == "__main__":
    try:
        check()
    except (RuntimeError, urllib.error.URLError, TimeoutError) as error:
        print("Backend check failed: " + str(error), file=sys.stderr)
        sys.exit(1)
