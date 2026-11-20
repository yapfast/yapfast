# Account settings

Settings > My Account contains a native Yapfast form with Overview, Profiles,
Connections, Authorized Apps, Devices and Nitro & Billing tabs. These are local
tabs of the account module, not extra entries in the shared settings navigation.
The module is `crates/yapfast-iced/src/settings/account.rs`; its only state is
the selected tab and whether the server chooser is open.

## Profiles in Yapfast

Overview keeps the existing account card, reconnect, custom status, Edit profile
and sign-out controls. Profiles > Edit main profile dispatches the existing
`Action::EditProfile`. Choose a server uses the servers already in the core's
snapshot, borrows their names and builds its choices only while expanded.
It offers available Discord servers whose existing menu permits
`EditServerProfile`, then dispatches that action with the selected server's id.
An unavailable or unknown server from a stale choice is ignored. The core still
checks whether a profile may be edited.

Both editors use the existing profile loading, editing, validation, save, reset
and error handling. Their supported fields and Nitro restrictions are described
in [main profiles](people.md#editing-ones-main-profile) and
[server profiles](people.md#editing-ones-server-profile). Opening an editor
closes settings. A Discord identity is required for these controls; there is no
placeholder editor when none is linked. The editor's Connections help opens the
official account-connections guide rather than an unverified settings URL.

## Official workflows in the browser

The Open Discord in browser button uses `https://discord.com/app` through the
existing browser opener. It opens the web app, without promising to select a
settings page. Each tab says to choose the page there and shows the manual
navigation path. Help buttons contain "help" in their label and open the specific
official article. Explore Nitro on discord.com opens the public Nitro page.

| Tab | Workflow after opening Discord | Official source |
| --- | --- | --- |
| Overview | User Settings > My Account for username, email and phone changes, password and authentication | [Usernames](https://support.discord.com/hc/en-us/articles/12620128861463-New-Usernames-Display-Names), [email](https://support.discord.com/hc/en-us/articles/4423385681175-How-to-Change-your-Discord-Account-s-Email), [phone numbers](https://support.discord.com/hc/en-us/articles/4413460214807-How-to-Remove-a-Phone-Number-From-a-Discord-Account), [passwords](https://support.discord.com/hc/en-us/articles/218410947-How-to-Reset-or-Change-Your-Forgotten-Password-on-Discord), [authentication](https://support.discord.com/hc/en-us/articles/219576828-Setting-up-Multi-Factor-Authentication) |
| Connections | User Settings > Connections for external accounts | [Account Connections FAQ](https://support.discord.com/hc/en-us/articles/32330173689623-Account-Connections-on-Discord-FAQ) |
| Authorized Apps | User Settings > Authorized Apps to review apps and permissions | [Discord's account-security guide](https://support.discord.com/hc/en-us/articles/24160905919511-My-Discord-Account-was-Hacked-or-Compromised) |
| Devices | User Settings > Devices to review signed-in devices | [Official Devices announcement](https://discord.com/blog/discord-update-november-1-2022-changelog); the tab's Login help opens the [QR login guide](https://support.discord.com/hc/en-us/articles/360039213771-QR-Code-Login-FAQ) |
| Nitro & Billing | User Settings > Subscriptions for plans and renewals; User Settings > Billing for payment methods, history and documents | [Managing Subscriptions and Billing FAQ](https://support.discord.com/hc/en-us/articles/360017693772-Managing-Subscriptions-and-Billing-FAQ), [public Nitro page](https://discord.com/nitro) |

## Boundaries and verification

Visiting these tabs sends no backend request. They neither fetch nor display
email addresses, phone numbers, authorized-app lists, connected-account lists,
device sessions, subscription balances, renewal dates or payment data. They
collect no credentials, authentication codes or payment details. Account
security, connection changes, authorization changes, remote session logout,
purchases, subscription changes and cancellations remain manual workflows in
Discord. There are no new account deletion or disabling controls.

The existing profile API and local fixtures cover the editors; no new Discord
endpoint, command, core state or fake route is needed. The tests in
`crates/yapfast-iced/src/tests/settings_account.rs` exercise tab navigation
without backend traffic, main and server editor dispatch, missing-identity and
empty-server cases, stale server choices, explicit manual navigation, and the
labels and destinations of browser actions. Browser opening is suppressed by
the existing test configuration. These tests use preview state and the local
Discord snapshot fixture, never a live account.
