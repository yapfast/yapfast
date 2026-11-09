;; Bulk-copy echo guest: a measurement twin of echo.wat.
;;
;; Test source only: tests assemble this file with the `wat` crate. No `.wasm`
;; output is checked in.
;;
;; Exports (identical to echo.wat):
;;   yf_activate   -> 0
;;   yf_deactivate -> 0
;;   echo          -> returns the call input unchanged
;;
;; Why this file exists
;; -------------------
;; `echo.wat` copies one byte at a time, so every input byte costs two kernel
;; calls (`input_load_u8` + `store_u8`). `bulk_echo.wat` copies complete 8-byte
;; chunks with `input_load_u64` / `store_u64` and uses the byte-wide pair only
;; for the trailing `len % 8` bytes. Both guests allocate a fresh output buffer
;; and return every input byte unchanged, so a latency run against the two
;; isolates what the guest's per-byte loop costs from the parent/helper IPC cost
;; that surrounds the call. Neither fixture implies anything about a production
;; copy path; `bulk_echo.wat` is a benchmark control, not a recommended guest.
;;
;; Byte order is not an issue: `input_load_u64` and `store_u64` are inverse
;; kernel operations, so a load/store round trip reproduces the input bytes
;; exactly regardless of the kernel's internal endianness. That is asserted, not
;; assumed — the fixture is validated against the real Extism 1.41 kernel with
;; asymmetric and non-ASCII payloads (see README.md).
;;
;; Imports are Extism 1.41's kernel ABI (`extism:host/env`); no WASI and no
;; custom host functions.

(module
  (import "extism:host/env" "input_length" (func $input_length (result i64)))
  (import "extism:host/env" "input_load_u64" (func $input_load_u64 (param i64) (result i64)))
  (import "extism:host/env" "input_load_u8" (func $input_load_u8 (param i64) (result i32)))
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "store_u64" (func $store_u64 (param i64 i64)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))

  ;; Copies `len` bytes of the call input into a fresh guest buffer and sets it
  ;; as the output: 8-byte chunks first, then the trailing `len % 8` bytes.
  (func $copy (param $len i64) (local $i i64) (local $full i64) (local $out i64)
    ;; $full = len & ~7, i.e. the largest multiple of 8 that fits in the input.
    (local.set $full (i64.and (local.get $len) (i64.const -8)))
    (local.set $out (call $alloc (local.get $len)))

    (block $bulk_done
      (loop $bulk
        (br_if $bulk_done (i64.ge_u (local.get $i) (local.get $full)))
        (call $store_u64
          (i64.add (local.get $out) (local.get $i))
          (call $input_load_u64 (local.get $i)))
        (local.set $i (i64.add (local.get $i) (i64.const 8)))
        (br $bulk)))

    (block $tail_done
      (loop $tail
        (br_if $tail_done (i64.ge_u (local.get $i) (local.get $len)))
        (call $store_u8
          (i64.add (local.get $out) (local.get $i))
          (call $input_load_u8 (local.get $i)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $tail)))

    (call $output_set (local.get $out) (local.get $len)))

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "echo") (result i32)
    (call $copy (call $input_length))
    (i32.const 0)))
