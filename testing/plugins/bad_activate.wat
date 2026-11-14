;; A plugin that refuses to start, one way rather than by trapping: Extism reads
;; an export's status code, and anything but 0 is a failed call, here the
;; `Returned non-zero exit code: 1` that the runtime reports as
;; FaultKind::ActivateFailed. The end-to-end test expects that fault and a
;; helper that is still running.
(module
  (func (export "yf_activate") (result i32) (i32.const 1))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
