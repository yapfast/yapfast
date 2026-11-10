//! Whether a file is a module this helper can run, checked before it is
//! compiled.
//!
//! Two questions are cheap to answer by reading the binary and expensive to
//! answer after handing it to the compiler: is it a WebAssembly module at all,
//! and does it import anything nobody provides. The split and export sections
//! name everything that decides both, so one pass over the file — at load time,
//! never per call — replaces an instantiation error several layers down inside
//! wasmtime with a fault that names the offending import.
//!
//! The `\0asm` magic is checked rather than left to the runtime because
//! wasmtime is built with the WAT feature: without the check a `.wat` file, or
//! a manifest that happens to sit at the path the parent wrote, would compile
//! into something that then fails in a way nobody can act on.

use std::path::Path;

use yapfast_plugin_api::limits::MAX_WASM_BYTES;
use yapfast_plugin_api::message::{Fault, FaultKind, IMPORTS};

/// The exports the helper calls, in the order of [`Shape::present`]'s bits.
///
/// The ABI also names `yf_decorate`, which no call path reaches in this build,
/// so a guest that leaves it out still runs and one that exports it is not
/// refused.
pub const REQUIRED_EXPORTS: [&str; 5] = [
    "yf_activate",
    "yf_deactivate",
    "yf_event",
    "yf_hook",
    "yf_command",
];

/// The host module Extism defines its own kernel functions in.
const KERNEL_MODULE: &str = "extism:host/env";
/// The host module the two functions of this ABI are defined in.
const HOST_MODULE: &str = "extism:host/user";
/// The two module names a WASI preview 1 build imports from.
const WASI_MODULES: [&str; 2] = ["wasi_snapshot_preview1", "wasi_unstable"];

const MAGIC: [u8; 4] = *b"\0asm";

/// What a module exports of what this helper needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    present: u8,
}

impl Shape {
    /// The required exports the module doesn't have.
    pub fn missing_exports(&self) -> Vec<&'static str> {
        REQUIRED_EXPORTS
            .iter()
            .enumerate()
            .filter(|(bit, _)| self.present & (1 << bit) == 0)
            .map(|(_, name)| *name)
            .collect()
    }
}

/// Reads the module at `path` and checks it against what this build provides.
pub fn check_file(path: &Path, wasi: bool) -> Result<Shape, Fault> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| bad(format!("`{}` can't be read: {error}", path.display())))?;
    if metadata.len() > MAX_WASM_BYTES {
        return Err(bad(format!(
            "`{}` is {} bytes; a module is at most {MAX_WASM_BYTES}",
            path.display(),
            metadata.len()
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|error| bad(format!("`{}` can't be read: {error}", path.display())))?;
    check(&bytes, wasi).map_err(|fault| bad(format!("`{}`: {}", path.display(), fault.reason)))
}

/// Checks the bytes of a module, with the sections that matter.
pub fn check(bytes: &[u8], wasi: bool) -> Result<Shape, Fault> {
    if !bytes.starts_with(&MAGIC) {
        return Err(bad(
            "it doesn't start with the `\\0asm` magic, so it is not a compiled module; \
             a `.wat` and a manifest are both refused"
                .to_owned(),
        ));
    }
    // The magic alone accepts anything called WebAssembly: the four bytes after
    // it name the version, and this helper speaks the core binary format's 1.
    // A component-model module or a truncated download is refused here, where
    // the sentence can say so.
    match bytes.get(4..8) {
        Some([0x01, 0x00, 0x00, 0x00]) => {}
        Some(other) => {
            return Err(bad(format!(
                "it is WebAssembly but names version {:02x?}, and this helper runs version 1",
                other
            )));
        }
        None => {
            return Err(bad(
                "it is shorter than a WebAssembly header, so it was not written whole".to_owned(),
            ));
        }
    }
    // The magic and the version are the first eight bytes; the sections follow.
    let mut reader = Reader::new(bytes.get(8..).unwrap_or_default());
    let mut shape = Shape::default();
    while let Some(id) = reader.byte() {
        let size = reader.u32().ok_or_else(|| truncated("section length"))?;
        let body = reader
            .take(size as usize)
            .ok_or_else(|| truncated("section"))?;
        match id {
            SECTION_IMPORT => check_imports(body, wasi)?,
            SECTION_EXPORT => check_exports(body, &mut shape)?,
            _ => {}
        }
    }
    Ok(shape)
}

const SECTION_IMPORT: u8 = 2;
const SECTION_EXPORT: u8 = 7;

/// Reads the import section: every import must be one something provides.
fn check_imports(body: &[u8], wasi: bool) -> Result<(), Fault> {
    let mut reader = Reader::new(body);
    let count = reader.u32().ok_or_else(|| truncated("import count"))?;
    for _ in 0..count {
        let module = reader.name().ok_or_else(|| truncated("import module"))?;
        let field = reader.name().ok_or_else(|| truncated("import name"))?;
        let kind = reader.byte().ok_or_else(|| truncated("import kind"))?;
        check_import(module, field, kind, wasi)?;
        skip_description(&mut reader, kind)?;
    }
    Ok(())
}

/// Checks one import against what this build links.
///
/// `extism:host/env` is the runtime's own kernel, which Extism validates when
/// it links; the two functions this ABI adds live beside it in
/// `extism:host/user`, and a name there that isn't one of them is a PDK built
/// for another API version, which is worth saying out loud. WASI's imports are
/// only there when the manifest asked for WASI, because a module that needs it
/// and doesn't get it traps on its first write instead of failing to link.
fn check_import(module: &[u8], field: &[u8], kind: u8, wasi: bool) -> Result<(), Fault> {
    let module_name = String::from_utf8_lossy(module);
    let field_name = String::from_utf8_lossy(field);
    let allowed = match &*module_name {
        KERNEL_MODULE => true,
        HOST_MODULE => IMPORTS.iter().any(|import| field_name == *import),
        other if WASI_MODULES.contains(&other) => wasi,
        _ => false,
    };
    if allowed && !is_function(kind) {
        return Err(bad(format!(
            "it imports `{module_name}::{field_name}`, which is not a function, and functions are \
             all this build provides"
        )));
    }
    if allowed {
        return Ok(());
    }
    let why = if WASI_MODULES.contains(&&*module_name) {
        "its manifest doesn't ask for WASI"
    } else {
        "nothing provides it"
    };
    Err(bad(format!(
        "it imports `{module_name}::{field_name}`, and {why}"
    )))
}

/// Whether a definition is a function, which is all this build provides.
fn is_function(kind: u8) -> bool {
    kind == 0
}

/// Skips an import's type so the next one can be read.
fn skip_description(reader: &mut Reader<'_>, kind: u8) -> Result<(), Fault> {
    let short = || truncated("import section");
    match kind {
        // A function: the index of its type.
        0 => {
            reader.u32().ok_or_else(short)?;
        }
        // A table: its element type, then its limits.
        1 => {
            reader.byte().ok_or_else(short)?;
            skip_limits(reader)?;
        }
        // A memory: its limits alone.
        2 => skip_limits(reader)?,
        // A global: its value type and whether it is mutable.
        3 => {
            reader.byte().ok_or_else(short)?;
            reader.byte().ok_or_else(short)?;
        }
        // A tag: its attribute and the index of its type.
        4 => {
            reader.byte().ok_or_else(short)?;
            reader.u32().ok_or_else(short)?;
        }
        _ => {
            return Err(bad(format!(
                "its import section names kind {kind}, which WebAssembly doesn't define"
            )));
        }
    }
    Ok(())
}

/// Skips a table's or memory's limits: a flag byte, a minimum, and a maximum
/// when the flag says one follows.
fn skip_limits(reader: &mut Reader<'_>) -> Result<(), Fault> {
    let flags = reader.byte().ok_or_else(|| truncated("limits"))?;
    reader.u32().ok_or_else(|| truncated("limits"))?;
    if flags & 0x01 != 0 {
        reader.u32().ok_or_else(|| truncated("limits"))?;
    }
    Ok(())
}

fn check_exports(body: &[u8], shape: &mut Shape) -> Result<(), Fault> {
    let mut reader = Reader::new(body);
    let count = reader.u32().ok_or_else(|| truncated("export count"))?;
    for _ in 0..count {
        let name = reader.name().ok_or_else(|| truncated("export name"))?;
        let kind = reader.byte().ok_or_else(|| truncated("export kind"))?;
        reader.u32().ok_or_else(|| truncated("export index"))?;
        if kind != 0 {
            continue;
        }
        if let Some(bit) = REQUIRED_EXPORTS
            .iter()
            .position(|required| required.as_bytes() == name)
        {
            shape.present |= 1 << bit;
        }
    }
    Ok(())
}

fn bad(reason: String) -> Fault {
    Fault {
        kind: FaultKind::BadModule,
        reason,
    }
}

fn truncated(what: &str) -> Fault {
    bad(format!("the module ends in the middle of its {what}"))
}

/// A cursor over a byte slice that fails instead of panicking on a short read.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, at: 0 }
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    /// An unsigned LEB128 of at most five bytes, which is how every length,
    /// count and index in a module's sections is written.
    fn u32(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    /// A length-prefixed name.
    fn name(&mut self) -> Option<&'a [u8]> {
        let len = self.u32()?;
        self.take(len as usize)
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes an unsigned LEB128, the only integer encoding a module's
    /// sections use for lengths and counts.
    fn leb(mut value: u32, out: &mut Vec<u8>) {
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

    fn name(text: &str, out: &mut Vec<u8>) {
        leb(text.len() as u32, out);
        out.extend_from_slice(text.as_bytes());
    }

    fn section(id: u8, body: &[u8], out: &mut Vec<u8>) {
        out.push(id);
        leb(body.len() as u32, out);
        out.extend_from_slice(body);
    }

    /// A module with exactly the given imports and exports, each of which is an
    /// empty function. Enough of the format to exercise the reader.
    fn module(imports: &[(&str, &str)], exports: &[&str]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&[1, 0, 0, 0]);

        let mut types = Vec::new();
        leb(1, &mut types);
        types.push(0x60);
        leb(0, &mut types);
        leb(0, &mut types);
        section(1, &types, &mut out);

        if !imports.is_empty() {
            let mut body = Vec::new();
            leb(imports.len() as u32, &mut body);
            for (host, field) in imports {
                name(host, &mut body);
                name(field, &mut body);
                body.push(0x00);
                leb(0, &mut body);
            }
            section(SECTION_IMPORT, &body, &mut out);
        }

        if !exports.is_empty() {
            let mut body = Vec::new();
            leb(exports.len() as u32, &mut body);
            for export in exports {
                name(export, &mut body);
                body.push(0x00);
                leb(0, &mut body);
            }
            section(SECTION_EXPORT, &body, &mut out);
        }
        out
    }

    fn everything() -> Vec<u8> {
        module(
            &[
                (KERNEL_MODULE, "alloc"),
                (KERNEL_MODULE, "output_set"),
                (HOST_MODULE, "yf_call"),
                (HOST_MODULE, "yf_post"),
            ],
            &REQUIRED_EXPORTS,
        )
    }

    #[test]
    fn a_module_that_imports_what_this_build_links_is_accepted() {
        let shape = check(&everything(), false).expect("the module is one this build can run");
        assert!(shape.missing_exports().is_empty());
    }

    #[test]
    fn a_module_that_is_not_compiled_is_refused_before_the_compiler_sees_it() {
        let fault = check(b"(module)".as_slice(), false).expect_err("a WAT file is not a module");
        assert_eq!(fault.kind, FaultKind::BadModule);
        assert!(
            fault.reason.contains("asm"),
            "the reason names the magic: {}",
            fault.reason
        );
    }

    #[test]
    fn a_module_of_another_webassembly_version_is_refused_by_name() {
        let mut bytes = MAGIC.to_vec();
        // The component model's version, which the core format's runtime can't
        // run: the magic passes and the version is what refuses it.
        bytes.extend_from_slice(&[0x0d, 0x00, 0x01, 0x00]);
        let fault = check(&bytes, false).expect_err("another version is refused");
        assert_eq!(fault.kind, FaultKind::BadModule);
        assert!(
            fault.reason.contains("version"),
            "the reason names the version: {}",
            fault.reason
        );
    }

    #[test]
    fn a_truncated_header_says_the_file_is_short() {
        let fault = check(b"\0asm".as_slice(), false).expect_err("half a header is no header");
        assert_eq!(fault.kind, FaultKind::BadModule);
        assert!(
            fault.reason.contains("shorter than a WebAssembly header"),
            "{}",
            fault.reason
        );
    }

    #[test]
    fn an_unknown_import_is_reported_by_name() {
        let bytes = module(&[("somewhere.else", "yf_call")], &REQUIRED_EXPORTS);
        let fault = check(&bytes, false).expect_err("nothing provides that module");
        assert_eq!(fault.kind, FaultKind::BadModule);
        assert!(
            fault.reason.contains("somewhere.else::yf_call"),
            "the reason names the import: {}",
            fault.reason
        );
    }

    #[test]
    fn a_host_function_this_build_does_not_have_is_reported_by_name() {
        let bytes = module(&[(HOST_MODULE, "yf_decorate")], &REQUIRED_EXPORTS);
        let fault = check(&bytes, false).expect_err("only two host functions exist");
        assert!(
            fault.reason.contains("extism:host/user::yf_decorate"),
            "the reason names the import: {}",
            fault.reason
        );
    }

    #[test]
    fn wasi_is_imported_only_when_the_manifest_asks_for_it() {
        let bytes = module(&[("wasi_snapshot_preview1", "fd_write")], &REQUIRED_EXPORTS);
        let fault = check(&bytes, false).expect_err("a module without WASI needs no wasi imports");
        assert!(
            fault.reason.contains("wasi_snapshot_preview1::fd_write"),
            "the reason names the import: {}",
            fault.reason
        );
        assert!(check(&bytes, true).is_ok());
    }

    #[test]
    fn a_missing_export_is_named() {
        let bytes = module(&[], &["yf_activate", "yf_event", "yf_command"]);
        let shape = check(&bytes, false).expect("imports are fine");
        assert_eq!(shape.missing_exports(), vec!["yf_deactivate", "yf_hook"]);
    }

    #[test]
    fn a_module_that_ends_inside_a_section_is_refused_rather_than_read_past() {
        let mut bytes = everything();
        bytes.truncate(bytes.len() - 2);
        let fault = check(&bytes, false).expect_err("the export section is cut short");
        assert_eq!(fault.kind, FaultKind::BadModule);
    }

    #[test]
    fn an_import_section_that_ends_early_is_refused() {
        let mut body = Vec::new();
        leb(2, &mut body);
        name(HOST_MODULE, &mut body);
        name("yf_call", &mut body);
        body.push(0x00);
        leb(0, &mut body);
        // The second import is announced and then missing.
        name(HOST_MODULE, &mut body);
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&[1, 0, 0, 0]);
        section(SECTION_IMPORT, &body, &mut bytes);
        let fault = check(&bytes, false).expect_err("the section is short of an import");
        assert!(
            fault.reason.contains("ends in the middle"),
            "{}",
            fault.reason
        );
    }
}
