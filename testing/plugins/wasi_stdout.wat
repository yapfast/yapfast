;; Guest that writes to its own stdout through WASI.
;;
;; Test source only: tests assemble this file with the `wat` crate and hand the
;; resulting `.wasm` to the production loader. No `.wasm` output is checked in.
;;
;; Exports:
;;   yf_activate   -> 0   (activation must not allocate)
;;   yf_deactivate -> 0
;;   speak         -> writes one line to WASI stdout, sets no Extism output
;;
;; Extism hands a guest's WASI stdout to the helper's stdout when
;; `EXTISM_ENABLE_WASI_OUTPUT` is set, and the helper's stdout is the plugin
;; protocol's frame stream. The process test spawns the helper with that
;; variable set and `--wasi`; if the helper did not drop it, this module's line
;; lands between two frames and the parent cannot read the next frame.
;;
;; `fd_write` is WASI preview 1's; the iovec at offset 0 points at the line at
;; offset 16, and the written count goes at offset 8, which this guest ignores:
;; only the bytes on the stream matter.

(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "noise-from-wasi-stdout\n")

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "speak") (result i32)
    ;; iovec: { buf = 16, len = 23 }
    (i32.store (i32.const 0) (i32.const 16))
    (i32.store (i32.const 4) (i32.const 23))
    ;; fd_write(fd = 1, iovs = 0, iovs_len = 1, nwritten = 8)
    (drop (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))
    (i32.const 0)))
