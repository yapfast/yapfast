;; A plugin that activates, and then never leaves `yf_hook`: the plugin a budget
;; is for. The helper's watchdog holds the deadline of every running call and
;; cancels the instance when it passes, so a hook chain it is in answers with a
;; `deadline` fault against it and the subject as it was — the message goes out
;; unchanged, and only after the plugin's own budget, not for ever.
;;
;; A helper with no per-plugin deadline still answers, but only when the whole
;; chain's budget runs out; a helper with no deadline at all hangs on this
;; fixture, which is what the tests are checking for.
(module
  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook")
    (result i32)
    (loop $forever (br $forever))
    (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
