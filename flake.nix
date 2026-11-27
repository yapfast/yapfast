{
  description = "Yapfast, a native Discord client";

  # Development happens in nix/flake.nix's shells; this flake only builds the
  # app, so `nix profile add github:yapfast/yapfast` needs no checkout.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      # The commit and its day, which the build can't ask Git for in Nix's
      # sandbox; Settings and `yapfast --version` show them. A dirty tree has
      # no `rev`, and a release is built from one after scripts/set-version,
      # so its `dirtyRev` names the commit as well. Both are empty outside a
      # Git checkout.
      commit = self.rev or (nixpkgs.lib.removeSuffix "-dirty" (self.dirtyRev or ""));
      # lastModifiedDate is the commit's time in UTC, as 20261107123456.
      date = with builtins; let stamp = self.lastModifiedDate or ""; in
        if stringLength stamp < 8 then ""
        else "${substring 0 4 stamp}-${substring 4 2 stamp}-${substring 6 2 stamp}";
      build = import ./nix/package.nix;
    in {
      packages = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (system:
        let pkgs = import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; };
        in rec {
          yapfast = build { inherit pkgs commit date; };
          # The AppImage holds the build with release identification enabled.
          release = build { inherit pkgs commit date; release = true; };
          # The AppDir and runtime scripts/package-linux.py packs into the AppImage.
          appdir = import ./nix/appimage.nix { inherit pkgs; yapfast = release; };
          default = yapfast;
        });
    };
}
