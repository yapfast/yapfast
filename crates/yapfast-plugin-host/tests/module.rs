//! What a module file must be before Extism is given it.

mod support;

use std::io::Write;
use std::path::Path;

use yapfast_plugin_host::module::{self, MAGIC, MAX_MODULE_BYTES, ModuleError};

#[test]
fn an_absolute_wasm_module_passes() {
    let (_dir, path) = support::wasm_fixture("echo.wat");
    assert!(path.is_absolute(), "the fixture path is absolute");
    module::check(&path).expect("an assembled module passes");
}

#[test]
fn a_relative_path_is_refused() {
    // The helper resolves no paths of its own: the file the parent checked is
    // the file the helper loads.
    let error = module::check(Path::new("echo.wasm")).expect_err("a relative path is refused");
    assert!(
        matches!(&error, ModuleError::Relative(path) if path == Path::new("echo.wasm")),
        "expected Relative, got {error:?}"
    );
}

#[test]
fn a_wat_file_renamed_to_wasm_is_refused() {
    // The regression guard for the WAT trap: Extism would compile this text
    // happily, so the magic check is the only thing that stops it.
    let (_dir, path) = support::text_fixture("echo.wat");
    let error = module::check(&path).expect_err("text is not a module");
    assert!(
        matches!(&error, ModuleError::BadMagic(bytes) if bytes == b"(mod"),
        "expected BadMagic, got {error:?}"
    );
}

#[test]
fn a_foreign_version_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("version.wasm");
    let mut file = std::fs::File::create(&path).expect("create");
    file.write_all(&MAGIC).expect("magic");
    file.write_all(&[0x00, 0x00, 0x00, 0x00]).expect("version");
    file.write_all(&[0u8; 8]).expect("body");

    let error = module::check(&path).expect_err("version 0 is not version 1");
    assert!(
        matches!(&error, ModuleError::BadVersion(bytes) if bytes == &[0, 0, 0, 0]),
        "expected BadVersion, got {error:?}"
    );
}

#[test]
fn a_file_over_the_cap_is_refused_without_reading_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("huge.wasm");
    let file = std::fs::File::create(&path).expect("create");
    file.set_len(MAX_MODULE_BYTES + 1).expect("size");

    let error = module::check(&path).expect_err("over 8 MiB is refused");
    assert!(
        matches!(&error, ModuleError::TooLarge { len } if *len == MAX_MODULE_BYTES + 1),
        "expected TooLarge, got {error:?}"
    );
}

#[test]
fn a_file_shorter_than_a_header_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("short.wasm");
    std::fs::write(&path, b"\0as").expect("write");

    let error = module::check(&path).expect_err("three bytes is not a module");
    assert!(
        matches!(&error, ModuleError::TooShort { len } if *len == 3),
        "expected TooShort, got {error:?}"
    );
}

#[test]
fn a_missing_file_is_an_io_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let error = module::check(&dir.path().join("absent.wasm")).expect_err("no file");
    assert!(
        matches!(&error, ModuleError::Io(_)),
        "expected Io, got {error:?}"
    );
}
