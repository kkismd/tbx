pub mod cell;
pub mod constants;
pub mod dict;
pub mod error;
pub mod primitives;
pub mod vm;

/// Create a VM with all system primitives registered and sealed.
///
/// This is the standard way to obtain a ready-to-use VM. It calls
/// [`primitives::register_all`] to populate the system dictionary and
/// [`vm::VM::seal_sys`] to record the system boundary.
pub fn init_vm() -> vm::VM {
    let mut v = vm::VM::new();
    primitives::register_all(&mut v);
    v.seal_sys();
    v
}
