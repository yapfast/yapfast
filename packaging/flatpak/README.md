# Flatpak

`io.github.yapfast.Yapfast.yml` builds the Discord-only release in the Flatpak SDK, the way `nix/package.nix`
does. The build has no network, so it needs every crate listed in `cargo-sources.json`; that file is generated,
not tracked.

## Build and run it locally

Needs `flatpak`, `flatpak-builder` and the Flathub remote in your user installation
(`flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`).
From the repository's root:

```sh
flatpak install --user flathub org.flatpak.Builder
flatpak run --command=flatpak-cargo-generator org.flatpak.Builder Cargo.lock \
  -o packaging/flatpak/cargo-sources.json
cd packaging/flatpak
flatpak-builder --user --install-deps-from=flathub --force-clean build-dir io.github.yapfast.Yapfast.yml
flatpak-builder --run build-dir io.github.yapfast.Yapfast.yml yapfast --smoke-test
```

`--install-deps-from=flathub` pulls the 25.08 SDK and its rust-stable and llvm21 extensions on the first build.
Make `cargo-sources.json` again whenever `Cargo.lock` changes.

## Publish it on Flathub

Flathub builds and hosts it: the manifest and `cargo-sources.json` go to a new `flathub/io.github.yapfast.Yapfast`
repository through Flathub's submission process, and users get it with `flatpak install flathub
io.github.yapfast.Yapfast`.

## Or host your own repository

A Flatpak repository is a directory of static files, so any web server can host one. Below, `example.org`,
`/var/www/flatpak` and the remote name `yapfast` stand for your own.

1. A signing key, once. Flatpak checks every download against it:

   ```sh
   gpg --quick-gen-key 'Yapfast Flatpak <flatpak@example.org>' rsa4096 sign never
   KEY=<the key's 40-character fingerprint, from gpg --list-keys>
   ```

2. Build and export into a repository, signed (the first line creates the repository, once):

   ```sh
   ostree init --mode=archive --repo=repo
   flatpak-builder --user --install-deps-from=flathub --force-clean \
     --repo=repo --gpg-sign="$KEY" build-dir io.github.yapfast.Yapfast.yml
   flatpak build-update-repo --gpg-sign="$KEY" --generate-static-deltas repo
   ```

   Run both again for each release; `build-update-repo` refreshes the signed summary that clients read first.

3. The file users add the repository with, `yapfast.flatpakrepo`:

   ```sh
   cat > yapfast.flatpakrepo <<EOF
   [Flatpak Repo]
   Title=Yapfast
   Url=https://example.org/flatpak/repo/
   Homepage=https://yapfast.com
   GPGKey=$(gpg --export "$KEY" | base64 -w0)
   EOF
   ```

4. Copy both to the server, the repository with `--delete` so it matches the export:

   ```sh
   rsync -a --delete repo/ example.org:/var/www/flatpak/repo/
   rsync -a yapfast.flatpakrepo example.org:/var/www/flatpak/
   ```

5. Serve the directory as plain files. With nginx, inside the site's `server` block:

   ```nginx
   location /flatpak/ {
       alias /var/www/flatpak/;
   }
   ```

Users then add it and install:

```sh
flatpak remote-add --user --if-not-exists yapfast https://example.org/flatpak/yapfast.flatpakrepo
flatpak install --user yapfast io.github.yapfast.Yapfast
```

The runtime still comes from Flathub, so users need the Flathub remote too.

## Known gap

The tray icon does not show inside the sandbox; the manifest's comment above `modules:` says why.
