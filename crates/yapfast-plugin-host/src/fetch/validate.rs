//! Checking what a fetched plugin declares, and what its module can reach.
//!
//! Nothing here runs the module: the manifest is read as JSON, the module's
//! header is read for its size and magic, and its import section is walked to
//! see what it would call. A module that imports a host function the ABI doesn't
//! offer is refused with that import named, because the interesting question at
//! install time is what a plugin can reach, not whether it compiles.
//!
//! The module is hashed as it is read, in chunks, so a fetched module is never
//! held whole in this process.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};
use yapfast_plugin_api::limits;
use yapfast_plugin_api::manifest::PluginManifest;
use yapfast_plugin_api::message;

use super::MANIFEST_FILE;

/// The largest manifest read. A manifest is metadata: commands, settings and a
/// list of permissions, which runs to kilobytes, not megabytes.
const MAX_MANIFEST_BYTES: u64 = 1 << 20;

/// The largest import section read. A module importing a thousand functions is
/// already beyond what the ABI offers.
const MAX_IMPORT_BYTES: u64 = 1 << 20;

/// The module namespace Extism gives the host functions a guest declares, which
/// is where [`message::IMPORTS`] arrive.
const HOST_MODULE: &str = "extism:host/user";

/// The WASI namespaces a module may import from, when its manifest asks for
/// WASI: the two names the snapshots have shipped under.
const WASI_MODULES: [&str; 2] = ["wasi_snapshot_preview1", "wasi_unstable"];

/// What a plugin declares and what its module holds, once both are checked.
#[derive(Clone, Debug)]
pub struct Checked {
    /// The manifest, parsed and accepted by its own rules.
    pub manifest: PluginManifest,
    /// The SHA-256 of the manifest's text, as it was written in the repository.
    pub manifest_sha256: String,
    /// The SHA-256 of the module's bytes, when the plugin has one.
    pub wasm_sha256: Option<String>,
}

/// Reads the manifest in `dir`, then checks the module it names.
pub fn check(dir: &Path, manifest_text: &str) -> anyhow::Result<Checked> {
    let size = manifest_text.len() as u64;
    if size > MAX_MANIFEST_BYTES {
        bail!("`{MANIFEST_FILE}` is {size} bytes, over the {MAX_MANIFEST_BYTES} a manifest may be");
    }
    let manifest = PluginManifest::parse(manifest_text)
        .map_err(|error| anyhow::anyhow!("`{MANIFEST_FILE}` was refused: {error}"))?;
    let manifest_sha256 = sha256_bytes(manifest_text.as_bytes());

    let Some(wasm) = manifest
        .wasm
        .clone()
        .and_then(|path| manifest.resolve(&path))
    else {
        // A plugin that only names tweaks, commands or settings has no module;
        // there is nothing to load and nothing to check.
        return Ok(Checked {
            manifest,
            manifest_sha256,
            wasm_sha256: None,
        });
    };
    let path = dir.join(&wasm);
    let shown = wasm.display().to_string();
    let bytes = std::fs::metadata(&path)
        .with_context(|| format!("`{shown}` isn't in the plugin"))?
        .len();
    if bytes > limits::MAX_WASM_BYTES {
        bail!(
            "`{shown}` is {bytes} bytes; a module is at most {} MiB",
            limits::MAX_WASM_BYTES >> 20
        );
    }
    check_module(&path, &shown, manifest.wasi)?;
    let wasm_sha256 =
        sha256_file(&path).with_context(|| format!("`{shown}` couldn't be read for its hash"))?;
    Ok(Checked {
        manifest,
        manifest_sha256,
        wasm_sha256: Some(wasm_sha256),
    })
}

impl Checked {
    /// The address of this install in the store: the module's hash and the
    /// manifest's, or only the manifest's for a plugin without a module.
    pub fn address(&self) -> String {
        super::store::address(self.wasm_sha256.as_deref(), &self.manifest_sha256)
    }
}

/// Checks a module's header and the host functions it imports.
fn check_module(path: &Path, shown: &str, wasi: bool) -> anyhow::Result<()> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("`{shown}` couldn't be read"))?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .with_context(|| format!("`{shown}` is shorter than a module's header"))?;
    if &header[..4] != b"\0asm" {
        bail!("`{shown}` isn't a WebAssembly module: it doesn't start with the `\\0asm` magic");
    }
    if header[4..] != [1, 0, 0, 0] {
        bail!(
            "`{shown}` is WebAssembly of version {}.{}, and this build runs version 1",
            header[4],
            header[5]
        );
    }

    let mut position = 8u64;
    loop {
        let mut id = [0u8; 1];
        if file.read(&mut id).context("the module couldn't be read")? == 0 {
            // No import section at all: a module that calls nothing.
            return Ok(());
        }
        let size = u64::from(
            read_leb(&mut file).with_context(|| format!("`{shown}` isn't valid WebAssembly"))?,
        );
        position += 1 + size;
        if id[0] == 2 {
            if size > MAX_IMPORT_BYTES {
                bail!(
                    "`{shown}` declares more than {MAX_IMPORT_BYTES} bytes of imports, which is \
                     beyond anything the plugin ABI offers"
                );
            }
            let mut body = vec![0u8; size as usize];
            file.read_exact(&mut body)
                .with_context(|| format!("`{shown}` ends inside its import section"))?;
            return check_imports(&body, shown, wasi);
        }
        file.seek(SeekFrom::Current(size as i64))
            .with_context(|| format!("`{shown}` ends inside a section"))?;
        if position
            >= file
                .metadata()
                .context("the module couldn't be measured")?
                .len()
        {
            return Ok(());
        }
    }
}

/// Refuses an import the ABI doesn't offer, naming it.
fn check_imports(body: &[u8], shown: &str, wasi: bool) -> anyhow::Result<()> {
    let mut reader = Reader::new(body);
    let count = reader.leb().unwrap_or(0);
    for _ in 0..count {
        let (Some(module), Some(name)) = (reader.name(), reader.name()) else {
            bail!("`{shown}` ends inside its import section");
        };
        skip_import(&mut reader).with_context(|| format!("`{shown}` isn't valid WebAssembly"))?;
        let offered = if WASI_MODULES.contains(&module) {
            wasi
        } else {
            module == HOST_MODULE && message::IMPORTS.contains(&name)
        };
        if !offered {
            let because = if WASI_MODULES.contains(&module) {
                format!(
                    "`{module}.{name}` needs WASI, and `{MANIFEST_FILE}` doesn't say `\"wasi\": true`"
                )
            } else if module == HOST_MODULE {
                format!(
                    "`{module}.{name}` isn't a host function Yapfast provides; a plugin may call \
                     {}",
                    message::IMPORTS.join(", ")
                )
            } else {
                format!(
                    "`{module}.{name}` comes from a module Yapfast doesn't provide; a plugin may \
                     import {} from `{HOST_MODULE}`{}",
                    message::IMPORTS.join(", "),
                    if wasi { ", and WASI" } else { "" }
                )
            };
            bail!("`{shown}` imports {because}");
        }
    }
    Ok(())
}

/// Steps over one import's kind and its type reference.
fn skip_import(reader: &mut Reader<'_>) -> Option<()> {
    match reader.bytes(1)?[0] {
        0x00 => reader.leb().map(|_| ()),
        0x01 => {
            reader.bytes(1)?;
            skip_limits(reader)
        }
        0x02 => skip_limits(reader),
        0x03 => {
            reader.bytes(2)?;
            Some(())
        }
        0x04 => {
            reader.bytes(1)?;
            reader.leb().map(|_| ())
        }
        _ => None,
    }
}

/// Steps over a table's or memory's limits.
fn skip_limits(reader: &mut Reader<'_>) -> Option<()> {
    let flags = reader.bytes(1)?[0];
    reader.leb()?;
    if flags & 1 == 1 {
        reader.leb()?;
    }
    Some(())
}

/// Reads a section body, which a module's own lengths bound.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn leb(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in 0..5 {
            let byte = self.bytes(1)?[0];
            value |= u32::from(byte & 0x7f) << (shift * 7);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn name(&mut self) -> Option<&'a str> {
        let length = self.leb()? as usize;
        std::str::from_utf8(self.bytes(length)?).ok()
    }
}

/// Reads a section id's size, which follows it in the module file.
fn read_leb(file: &mut std::fs::File) -> std::io::Result<u32> {
    let mut value = 0u32;
    for shift in 0..5 {
        let mut byte = [0u8; 1];
        file.read_exact(&mut byte)?;
        value |= u32::from(byte[0] & 0x7f) << (shift * 7);
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "a length never ended",
    ))
}

/// The SHA-256 of a file, read in chunks so it is never held whole.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 << 10];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// The SHA-256 of a piece of text, lowercase hex.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Scratch;

    const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    /// A module whose import section names `imports`, as a build would emit it.
    fn module(imports: &[(&str, &str)]) -> Vec<u8> {
        let mut out = HEADER.to_vec();
        if imports.is_empty() {
            return out;
        }
        let mut body = Vec::new();
        leb(&mut body, imports.len() as u32);
        for (namespace, name) in imports {
            name_bytes(&mut body, namespace);
            name_bytes(&mut body, name);
            body.push(0x00);
            leb(&mut body, 0);
        }
        out.push(2);
        leb(&mut out, body.len() as u32);
        out.extend_from_slice(&body);
        out
    }

    fn leb(out: &mut Vec<u8>, mut value: u32) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn name_bytes(out: &mut Vec<u8>, name: &str) {
        leb(out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
    }

    #[test]
    fn a_module_that_calls_the_abi_is_accepted() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let path = scratch.path().join("plugin.wasm");
        std::fs::write(&path, module(&[(HOST_MODULE, "yf_call")])).unwrap();
        assert!(check_module(&path, "plugin.wasm", false).is_ok());
    }

    #[test]
    fn a_module_with_no_imports_is_accepted() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let path = scratch.path().join("plugin.wasm");
        std::fs::write(&path, module(&[])).unwrap();
        assert!(check_module(&path, "plugin.wasm", false).is_ok());
    }

    #[test]
    fn an_import_the_abi_does_not_offer_is_named() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let path = scratch.path().join("plugin.wasm");
        std::fs::write(&path, module(&[(HOST_MODULE, "yf_delete_everything")])).unwrap();
        let error = check_module(&path, "plugin.wasm", false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("yf_delete_everything"), "{error}");
    }

    #[test]
    fn another_module_s_imports_are_refused_and_named() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let path = scratch.path().join("plugin.wasm");
        std::fs::write(&path, module(&[("extism:host/env", "http_request")])).unwrap();
        let error = check_module(&path, "plugin.wasm", false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("extism:host/env.http_request"), "{error}");
    }

    #[test]
    fn wasi_is_read_only_when_the_manifest_asks() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let path = scratch.path().join("plugin.wasm");
        std::fs::write(&path, module(&[("wasi_snapshot_preview1", "fd_write")])).unwrap();
        assert!(check_module(&path, "plugin.wasm", true).is_ok());
        let error = check_module(&path, "plugin.wasm", false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("fd_write"), "{error}");
    }

    #[test]
    fn something_that_is_not_wasm_is_refused_as_such() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let path = scratch.path().join("plugin.wasm");
        std::fs::write(&path, b"#!/bin/sh\necho hi\n").unwrap();
        let error = check_module(&path, "plugin.wasm", false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("magic"), "{error}");
    }

    #[test]
    fn hashes_match_a_known_vector() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reads_a_manifest_and_refuses_a_path_out_of_the_directory() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let text = r#"{
            "api": 1,
            "id": "com.example.escapes",
            "name": "Escapes",
            "version": "1.0.0",
            "wasm": "../x.wasm"
        }"#;
        let error = check(scratch.path(), text).unwrap_err().to_string();
        assert!(error.contains("wasm"), "{error}");
    }

    #[test]
    fn a_manifest_without_a_module_needs_no_files() {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let text = r#"{
            "api": 1,
            "id": "com.example.no-mosaic",
            "name": "No Mosaic",
            "version": "1.0.0",
            "tweaks": { "messages.attachments.mosaic": false }
        }"#;
        let checked = check(scratch.path(), text).unwrap();
        assert!(checked.wasm_sha256.is_none());
        assert!(checked.address().len() == 64);
    }
}
