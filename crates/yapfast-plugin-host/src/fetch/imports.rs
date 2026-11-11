//! What a fetched module can reach, before the parent is told it exists.
//!
//! Nothing here runs the module: its header is read for the magic and the version,
//! and its import section is walked to see what it would call. A module that
//! imports a host function the ABI doesn't offer is refused with that import
//! named, because the question at install time is what a plugin can reach, not
//! whether it compiles. See `PLAN.md` §8.5 step 4.
//!
//! The module is already in memory — the fetch read it out of a git object — and
//! is at most 8 MiB, so this walks the bytes it was handed rather than reading a
//! file a second time.

use anyhow::{Context, bail};
use yapfast_plugin_api::message;

use super::MANIFEST_FILE;

/// The module namespace Extism gives the host functions a guest declares, which
/// is where [`message::IMPORTS`] arrive.
const HOST_MODULE: &str = "extism:host/user";

/// Extism's own namespace: memory, buffers and input/output, which every guest
/// built with a PDK imports and which gives no capability of its own. Yapfast's
/// host functions live in [`HOST_MODULE`] instead, and only the ABI's are
/// offered there.
const EXTISM_MODULE: &str = "extism:host/env";

/// The WASI namespaces a module may import from, when its manifest asks for
/// WASI: the two names the snapshots have shipped under.
const WASI_MODULES: [&str; 2] = ["wasi_snapshot_preview1", "wasi_unstable"];

/// The largest import section read. A module importing a thousand functions is
/// already beyond what the ABI offers.
const MAX_IMPORT_BYTES: u64 = 1 << 20;

/// Refuses a module that isn't WebAssembly version 1, or that imports something
/// Yapfast doesn't provide. `shown` is what the manifest called it, for the
/// sentence a person reads.
pub fn check(bytes: &[u8], shown: &str, wasi: bool) -> anyhow::Result<()> {
    if !bytes.starts_with(b"\0asm") {
        bail!("`{shown}` isn't a WebAssembly module: it doesn't start with the `\\0asm` magic");
    }
    if bytes.len() < 8 || bytes[4..8] != [1, 0, 0, 0] {
        bail!(
            "`{shown}` is a WebAssembly module of a version this build doesn't run: it reads {}",
            if bytes.len() < 8 {
                "past its header".to_owned()
            } else {
                format!("{}.{}", bytes[4], bytes[5])
            }
        );
    }
    let mut at = 8;
    while at < bytes.len() {
        let id = bytes[at];
        at += 1;
        let (size, read) =
            leb(&bytes[at..]).with_context(|| format!("`{shown}` isn't valid WebAssembly"))?;
        at += read;
        let end = at
            .checked_add(size as usize)
            .filter(|end| *end <= bytes.len())
            .with_context(|| format!("`{shown}` ends inside a section"))?;
        // Section 2 is the import section, and the first one is the only one.
        if id == 2 {
            if u64::from(size) > MAX_IMPORT_BYTES {
                bail!(
                    "`{shown}` declares more than {MAX_IMPORT_BYTES} bytes of imports, which is \
                     beyond anything the plugin ABI offers"
                );
            }
            return check_imports(&bytes[at..end], shown, wasi);
        }
        at = end;
    }
    // No import section at all: a module that calls nothing.
    Ok(())
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
        } else if module == EXTISM_MODULE {
            // Extism provides these itself, whatever the plugin asks of Yapfast.
            true
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
                    "`{module}.{name}` isn't a host function Yapfast provides; a plugin may call {}",
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

/// A section's length, and how many bytes of it that took.
fn leb(bytes: &[u8]) -> anyhow::Result<(u32, usize)> {
    let mut value = 0u32;
    for shift in 0..5 {
        let byte = *bytes
            .get(shift)
            .with_context(|| "a length never ended".to_owned())?;
        value |= u32::from(byte & 0x7f) << (shift * 7);
        if byte & 0x80 == 0 {
            return Ok((value, shift + 1));
        }
    }
    bail!("a length never ended")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(check(&module(&[(HOST_MODULE, "yf_call")]), "plugin.wasm", false).is_ok());
    }

    #[test]
    fn a_module_with_no_imports_is_accepted() {
        assert!(check(&module(&[]), "plugin.wasm", false).is_ok());
    }

    #[test]
    fn an_import_the_abi_does_not_offer_is_named() {
        let error = check(
            &module(&[(HOST_MODULE, "yf_delete_everything")]),
            "plugin.wasm",
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("yf_delete_everything"), "{error}");
    }

    /// Every guest a PDK builds imports Extism's own memory functions, so
    /// refusing them would refuse every real plugin.
    #[test]
    fn extism_s_own_functions_are_accepted() {
        let plugin = module(&[(EXTISM_MODULE, "alloc"), (HOST_MODULE, "yf_call")]);
        assert!(check(&plugin, "plugin.wasm", false).is_ok());
    }

    #[test]
    fn another_module_s_imports_are_refused_and_named() {
        let error = check(
            &module(&[("extism:host/vendor", "http_request")]),
            "plugin.wasm",
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("extism:host/vendor.http_request"), "{error}");
    }

    #[test]
    fn wasi_is_read_only_when_the_manifest_asks() {
        let wasi = module(&[("wasi_snapshot_preview1", "fd_write")]);
        assert!(check(&wasi, "plugin.wasm", true).is_ok());
        let error = check(&wasi, "plugin.wasm", false).unwrap_err().to_string();
        assert!(error.contains("fd_write"), "{error}");
    }

    #[test]
    fn something_that_is_not_wasm_is_refused_as_such() {
        let error = check(b"#!/bin/sh\necho hi\n", "plugin.wasm", false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("magic"), "{error}");
    }

    /// The fixtures the helper's end-to-end tests load are real modules: the
    /// checker must accept them, or nothing could be installed.
    #[test]
    fn the_repository_s_fixtures_are_accepted() {
        for name in ["echo.wasm", "trap.wasm", "quiet"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/plugins")
                .join(name);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let result = check(&bytes, name, false);
            assert!(result.is_ok(), "{name}: {result:?}");
        }
    }
}
