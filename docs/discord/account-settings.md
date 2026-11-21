# Account settings

Settings > My Account contains a native Yapfast form with Overview, Profiles,
Connections, Authorized Apps and Devices tabs. These are local tabs of the
account module, not extra entries in the shared settings navigation. The module
is `crates/yapfast-iced/src/settings/account.rs`; its state is the selected tab,
whether the server chooser is open and the username, email or password form
that is open. The lists and the progress of every request live in the core
(`yapfast_core::account_tools::AccountTools`, in `AppState::account_tools`).

Everything on these tabs is done inside Yapfast. There is no button that sends
you to Discord's website to finish the job, and no placeholder: what Yapfast
cannot do natively is not shown (see "Not in Yapfast" below).

## Overview

Overview keeps the account card, reconnect, custom status, Edit profile and
sign-out controls, and adds Account details: Username, Email and Password, each
with a Change button that opens a small form.

Reconnect asks the linked Discord gateway to retry during a connection outage,
subject to its existing rate limits. Switching Discord accounts keeps unsent
text, replies and staged files with their originating account for this run.

* Username and Email ask for the new value and the current password.
* Password asks for the new password twice and the current one. The Save button
  stays off until the two new passwords are the same.
* The change is `PATCH /users/@me` with the current `password` and one of
  `username`, `email` or `new_password`. Discord answers a password change with
  a new token; Yapfast keeps it (`service/session.rs::relogin`) so the account
  stays linked while Discord signs the other devices out.
* The current password is cleared from the form after each try, so a refused
  change asks for it again. A refusal's reason is shown under the form.
* Nothing is sent in the preview or while Discord is not connected.

## Profiles

Profiles > Edit main profile dispatches `Action::EditProfile`. Choose a server
uses the servers already in the core's snapshot and offers the available Discord
servers whose menu permits `EditServerProfile`. An unavailable or unknown server
from a stale choice is ignored. The editors are described in
[main profiles](people.md#editing-ones-main-profile) and
[server profiles](people.md#editing-ones-server-profile). The profile editor's
Connections row links to the Connections tab here (`settings::Event::Connections`).

## Authorized Apps

`GET /oauth2/tokens` lists the apps the account let in: name, description, icon
and scopes, each scope in words ("See your username and avatar"). Revoke is
`DELETE /oauth2/tokens/{id}` and removes the row when Discord agrees; when it
refuses, the row stays and the reason is shown.

## Devices

`GET /auth/sessions` lists where the account is signed in: platform, operating
system, location and when it last used the account. The device Yapfast runs on is
the one whose session hash the gateway's READY sent as `auth_session_id_hash`; it
is marked "This device" and has no Sign out button. Sign out is
`POST /auth/sessions/logout` with the session hashes; "Sign out all other devices"
shows when more than one other device is listed.

## Connections

`GET /users/@me/connections` lists the linked services. Each shows its name,
whether it is verified, and two switches: Display on profile and Display as
activity status (`PATCH /users/@me/connections/{type}/{id}`). Disconnect is
`DELETE` on the same path. Add a connection asks `GET /connections/{type}/authorize`
for the service's sign-in page, which opens in the browser: the provider's own
sign-in cannot be drawn inside Yapfast, Discord completes the link on its side,
and "Read the list again" shows the result. The services offered are
`yapfast_core::account_tools::SERVICES`.

## Not in Yapfast

These are not shown, because they cannot be done natively and a button that only
opens a web page is not acceptable:

* Phone numbers and multi-factor authentication (they need Discord's SMS and
  authenticator enrolment flows).
* Nitro and Boost purchases and subscription management, payment methods,
  billing history and invoices: real money is never spent from a blind call. Orb
  purchases and Shop items bought with Orbs are native; see the Shop docs.
* Account deletion and disabling.

## Verification

The discord endpoints and answer shapes above are written from Discord's client
behaviour and have not been run against a live account; the lists are parsed
tolerantly (`yapfast-discord/src/account_tools.rs` has unit tests for the
shapes). The core's tests cover busy keys, refusals and the unconnected
preview; `crates/yapfast-iced/src/tests/settings_account.rs` drives the tabs,
revoking, signing out, the switches, the forms and the profile editor's link
with the preview's fixtures, never a live account.
