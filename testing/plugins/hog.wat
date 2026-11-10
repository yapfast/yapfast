;; A plugin that eats its own memory. `yf_activate` grows linear memory 64
;; pages at a time until the instance's page limit refuses the growth; Extism's
;; limiter traps with the cause `oom` rather than handing back -1, and an
;; instance that can't shrink is discarded. The end-to-end test loads it with a
;; small page limit and expects a Fault of kind OutOfMemory.
;;
;; The memory has no declared maximum: with one, `memory.grow` would answer -1
;; instead of trapping, and the plugin would keep running at its ceiling.
(module
  (memory 1)
  (func (export "yf_activate")
    (result i32)
    (loop $grow
      (drop (memory.grow (i32.const 64)))
      (br $grow))
    (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
