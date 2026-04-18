use crate::cell::{Cell, ReturnFrame};
use crate::dict::{EntryKind, WordEntry};
use crate::error::TbxError;
use crate::vm::VM;

/// DROP — discard the top element of the data stack.
pub fn drop_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.pop()?;
    Ok(())
}

/// DUP — duplicate the top element of the data stack.
pub fn dup_prim(vm: &mut VM) -> Result<(), TbxError> {
    let top = vm.pop()?;
    vm.push(top.clone());
    vm.push(top);
    Ok(())
}

/// SWAP — exchange the top two elements of the data stack.
pub fn swap_prim(vm: &mut VM) -> Result<(), TbxError> {
    let a = vm.pop()?;
    let b = vm.pop()?;
    vm.push(a);
    vm.push(b);
    Ok(())
}

/// FETCH — fetch a value from an address and push it onto the stack.
pub fn fetch_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            let size = vm.dictionary.len();
            let value = vm
                .dictionary
                .get(a)
                .ok_or(TbxError::IndexOutOfBounds { index: a, size })?
                .clone();
            vm.push(value);
            Ok(())
        }
        Cell::StackAddr(a) => {
            // TODO: vm.bp との加算をvmにカプセル化したい。その中で範囲チェックも行う。
            let idx = vm.bp + a;
            let size = vm.data_stack.len();
            let value = vm
                .data_stack
                .get(idx)
                .ok_or(TbxError::IndexOutOfBounds { index: idx, size })?
                .clone();
            vm.push(value);
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// STORE — pop a value and an address, and store the value at the address.
pub fn store_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    let value = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            let size = vm.dictionary.len();
            *vm.dictionary
                .get_mut(a)
                .ok_or(TbxError::IndexOutOfBounds { index: a, size })? = value;
            Ok(())
        }
        Cell::StackAddr(a) => {
            let idx = vm.bp + a;
            let size = vm.data_stack.len();
            *vm.data_stack
                .get_mut(idx)
                .ok_or(TbxError::IndexOutOfBounds { index: idx, size })? = value;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// CALL — call an execution token (Xt).
pub fn call_prim(vm: &mut VM) -> Result<(), TbxError> {
    let xt = vm
        .dictionary
        .get(vm.pc + 1)
        .ok_or(TbxError::IndexOutOfBounds {
            index: vm.pc + 1,
            size: vm.dictionary.len(),
        })?;
    if let Cell::Xt(x) = xt {
        let pc = vm.pc + 2; // CALL instruction is 2 cells: opcode + xt
        let bp = vm.bp;
        let return_frame = ReturnFrame::Call { pc, bp };
        vm.return_stack.push(return_frame);
        vm.bp = vm.data_stack.len();
        match vm.headers[x.index()].kind {
            EntryKind::Word(offset) => vm.pc = offset,
            _ => {
                return Err(TbxError::TypeError {
                    expected: "callable (primitive or word)",
                    got: "non-callable",
                })
            }
        }
        Ok(())
    } else {
        Err(TbxError::TypeError {
            expected: "Xt",
            got: xt.type_name(),
        })
    }
}

pub fn exit_prim(vm: &mut VM) -> Result<(), TbxError> {
    let return_frame = vm.return_stack.pop().ok_or(TbxError::StackUnderflow)?;
    let ReturnFrame::Call { pc, bp } = return_frame;
    vm.pc = pc;
    vm.bp = bp;
    Ok(())
}

/// Register all stack primitives into the VM's dictionary.
pub fn register_all(vm: &mut VM) {
    vm.register(WordEntry::new_primitive("DROP", drop_prim));
    vm.register(WordEntry::new_primitive("DUP", dup_prim));
    vm.register(WordEntry::new_primitive("SWAP", swap_prim));
    vm.register(WordEntry::new_primitive("FETCH", fetch_prim));
    vm.register(WordEntry::new_primitive("STORE", store_prim));
    vm.register(WordEntry::new_primitive("CALL", call_prim));
    vm.register(WordEntry::new_primitive("EXIT", exit_prim));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    // --- drop_prim ---

    #[test]
    fn test_drop_removes_top() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        drop_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_drop_underflow() {
        let mut vm = VM::new();
        assert_eq!(drop_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- dup_prim ---

    #[test]
    fn test_dup_duplicates_top() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        dup_prim(&mut vm).unwrap();
        // Both copies must be on the stack; the original is below.
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_dup_underflow() {
        let mut vm = VM::new();
        assert_eq!(dup_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- swap_prim ---

    #[test]
    fn test_swap_exchanges_top_two() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        swap_prim(&mut vm).unwrap();
        // After swap: 1 is on top, 2 is below.
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
        assert_eq!(vm.pop(), Ok(Cell::Int(2)));
    }

    #[test]
    fn test_swap_underflow_empty() {
        let mut vm = VM::new();
        assert_eq!(swap_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_swap_underflow_one_element() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        assert_eq!(swap_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- register_all ---

    #[test]
    fn test_register_all_registers_drop_dup_swap() {
        let mut vm = VM::new();
        register_all(&mut vm);
        assert!(vm.lookup("DROP").is_some());
        assert!(vm.lookup("DUP").is_some());
        assert!(vm.lookup("SWAP").is_some());
    }

    #[test]
    fn test_register_all_drop_is_callable() {
        let mut vm = VM::new();
        register_all(&mut vm);
        let xt = vm.lookup("DROP").unwrap();
        vm.push(Cell::Int(99));
        if let crate::dict::EntryKind::Primitive(f) = vm.headers[xt.index()].kind {
            f(&mut vm).unwrap();
        } else {
            panic!("DROP is not a Primitive");
        }
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_fetch_dict_addr() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::Int(123)); // dict[0] = 123
        vm.push(Cell::DictAddr(0));
        fetch_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(123)));
    }

    #[test]
    fn test_fetch_stack_addr() {
        // This test also verifies that fetch_prim correctly adds vm.bp to the address.
        let mut vm = VM::new();
        vm.push(Cell::Int(10)); // data_stack[0] = 10
        vm.push(Cell::Int(20)); // data_stack[1] = 20
        vm.bp = 1; // base pointer at index 1
        vm.push(Cell::StackAddr(0)); // address of data_stack[bp + 0] = data_stack[1] = 20
        fetch_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_fetch_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)); // Not an address
        assert_eq!(
            fetch_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "address",
                got: "non-address"
            })
        );
    }

    #[test]
    fn test_fetch_underflow() {
        let mut vm = VM::new();
        assert_eq!(fetch_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_store_dict_addr() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::Int(0)); // dict[0] = 0
        vm.push(Cell::Int(123)); // value to store
        vm.push(Cell::DictAddr(0)); // address to store at
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.dictionary[0], Cell::Int(123));
    }

    #[test]
    fn test_store_stack_addr() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)); // data_stack[0] = 0
        vm.bp = 0;
        vm.push(Cell::Int(123)); // value to store
        vm.push(Cell::StackAddr(0)); // address to store at
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.data_stack[0], Cell::Int(123));
    }

    #[test]
    fn test_store_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(123)); // value to store
        vm.push(Cell::Int(0)); // Not an address
        assert_eq!(
            store_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "address",
                got: "non-address"
            })
        );
    }

    #[test]
    fn test_store_underflow() {
        let mut vm = VM::new();
        assert_eq!(store_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_store_underflow_one_value() {
        let mut vm = VM::new();
        vm.push(Cell::Int(123)); // value to store
        assert_eq!(store_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_call_and_exit() {
        let mut vm = VM::new();
        register_all(&mut vm); // Ensure CALL and EXIT are registered
        let dummy_xt = vm.register(WordEntry::new_word("TEST", 10));
        vm.dictionary.push(Cell::None);
        vm.dictionary.push(Cell::Xt(dummy_xt)); // code for TEST: CALL EXIT

        vm.pc = 0;
        call_prim(&mut vm).unwrap();
        assert_eq!(vm.pc, 10); // After CALL, pc should be at TEST's code
    }

    #[test]
    fn test_exit_restores_pc_bp() {
        let mut vm = VM::new();
        register_all(&mut vm); // Ensure CALL and EXIT are registered
        vm.return_stack.push(ReturnFrame::Call { pc: 42, bp: 99 });
        exit_prim(&mut vm).unwrap();
        assert_eq!(vm.pc, 42);
        assert_eq!(vm.bp, 99);
    }

    #[test]
    fn test_exit_underflow() {
        let mut vm = VM::new();
        assert_eq!(exit_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_call_type_error() {
        let mut vm = VM::new();
        register_all(&mut vm); // Ensure CALL and EXIT are registered
        vm.dictionary.push(Cell::None);
        vm.dictionary.push(Cell::Int(123)); // Not an Xt
        vm.pc = 0;
        assert_eq!(call_prim(&mut vm), Err(TbxError::TypeError {
            expected: "Xt",
            got: "Int"
        }));
    }
}