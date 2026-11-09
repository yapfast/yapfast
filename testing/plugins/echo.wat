;; Echo guest for activation and call tests.
;;
;; Test source only: tests assemble this file with the `wat` crate and hand the
;; resulting `.wasm` to the production loader. No `.wasm` output is checked in.
;;
;; Exports:
;;   yf_activate   -> 0
;;   yf_deactivate -> 0
;;   echo          -> returns the call input unchanged
;;
;; The imports are Extism 1.41's kernel ABI (`extism:host/env`); the module
;; needs no WASI and no custom host functions beyond the kernel.

(module
  (import "extism:host/env" "input_length" (func $input_length (result i64)))
  (import "extism:host/env" "input_load_u8" (func $input_load_u8 (param i64) (result i32)))
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))

  ;; Copies `len` bytes of the call input into a fresh guest buffer and sets it
  ;; as the output. The copy (rather than output_set on the input handle) keeps
  ;; the fixture independent of Extism's input/output aliasing rules.
  (func $copy (param $len i64) (local $i i64) (local $out i64)
    (local.set $out (call $alloc (local.get $len)))
    (block $done
      (loop $each
        (br_if $done (i64.ge_u (local.get $i) (local.get $len)))
        (call $store_u8
          (i64.add (local.get $out) (local.get $i))
          (call $input_load_u8 (local.get $i)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $each)))
    (call $output_set (local.get $out) (local.get $len)))

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "echo") (result i32)
    (call $copy (call $input_length))
    (i32.const 0)))
