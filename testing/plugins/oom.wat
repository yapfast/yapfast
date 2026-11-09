;; Allocation-fault guest: grows the Extism kernel memory past the plugin's
;; page cap.
;;
;; Test source only: tests assemble this file with the `wat` crate. No `.wasm`
;; output is checked in.
;;
;; Exports:
;;   yf_activate   -> 0   (activation must not allocate)
;;   yf_deactivate -> 0
;;   eat_memory    -> loops calling alloc until killed by the memory limiter
;;
;; Each alloc asks the Extism kernel for 1 MiB and the handle is dropped, so the
;; kernel's own allocator keeps growing. Extism 1.41 attaches a
;; wasmtime::ResourceLimiter to the store (max_pages * 64 KiB, reset after
;; instantiation); the grow that crosses that budget returns Err("oom") from
;; the limiter, which Extism reports as root cause "oom".
;;
;; 32 allocations = 32 MiB, twice the 256-page (16 MiB) default, so the cap is
;; reached even with kernel and input overhead. The loop is bounded to keep a
;; misconfigured (unlimited) host from running it forever.

(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "eat_memory") (result i32)
    (local $i i32)
    (block $done
      (loop $again
        (br_if $done (i32.ge_u (local.get $i) (i32.const 32)))
        (drop (call $alloc (i64.const 1048576)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $again)))
    (i32.const 0)))
