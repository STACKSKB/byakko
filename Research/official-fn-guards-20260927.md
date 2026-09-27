# Nia87 Fn edit restrictions, 2026-09-27

Read-only inspection used the already-authorized local official Driver 2.1.97
renderer against its original helper, using the same display shim and RPC proxy
described in `rgb-white-boundary-20260927.md`. No Confirm, Reset or other device
write control was used. The browser and the three temporary processes were
closed after inspection.

The FnSetting keyboard marks these physical positions red and labels them
“System Key”: backtick, 1–4, Backspace, Insert, Delete, Z, X, C, V, right Shift,
Up, left Windows, Fn, right Ctrl, Left, Down and Right. Clicking Backspace
produced the explicit message “System keys cannot modified”. These correspond
to matrix slots 1, 7, 13, 19, 25, 79, 85, 86, 16, 22, 28, 34, 76, 82, 17,
59, 71, 77, 83 and 89. This is evidence of UI restrictions, not a claim about
firmware rejecting writes or a decoded meaning for every stored command.

Fn+Esc was cyan, identified as fnLock, and permitted changing the local Ctrl
checkbox draft. No save was performed. Separately, the user requested protecting
Fn+Esc in the pre-alpha because it is the factory-reset combination. Byakko
therefore also protects slot 0; this extra restriction is user policy rather
than an inference from the official interface.

Byakko describes protected positions per layer and checks them in portable
editing, macro assignment planning and Nia87 forward-write validation. Base
positions remain editable where previously supported; ordinary Fn positions
remain available for macros. Raw snapshots preserve every protected binding.
No vendor source is copied into the product.
