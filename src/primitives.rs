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
    let xt_cell = vm
        .dictionary
        .get(vm.pc + 1)
        .ok_or(TbxError::IndexOutOfBounds {
            index: vm.pc + 1,
            size: vm.dictionary.len(),
        })?;
    if let Cell::Xt(x) = xt_cell {
        let offset = match vm.headers[x.index()].kind {
            EntryKind::Word(offset) => offset,
            _ => {
                return Err(TbxError::TypeError {
                    expected: "callable (primitive or word)",
                    got: "non-callable",
                })
            }
        };
        let pc = vm.pc + 2; // CALL命令の次の命令のアドレス)
        let bp = vm.bp;
        let return_frame = ReturnFrame::Call { pc, bp };
        vm.return_stack.push(return_frame);
        vm.bp = vm.data_stack.len();
        vm.pc = offset;
        Ok(())
    } else {
        Err(TbxError::TypeError {
            expected: "Xt",
            got: xt_cell.type_name(),
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

pub fn add_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => vm.push(Cell::Int(x + y)),
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x + y)),
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 + y)),
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x + y as f64)),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    }
    Ok(())
}

pub fn sub_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => vm.push(Cell::Int(x - y)),
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x - y)),
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 - y)),
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x - y as f64)),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    }
    Ok(())
}

pub fn mul_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => vm.push(Cell::Int(x * y)),
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x * y)),
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 * y)),
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x * y as f64)),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    }
    Ok(())
}

#[allow(clippy::redundant_guards)] // Float(0.0) pattern also matches -0.0; use guard for clarity
pub fn div_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    match (a, b) {
        (Cell::Int(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Int(y)) => vm.push(Cell::Int(x / y)),
        (Cell::Float(_), Cell::Float(y)) if y == 0.0 => return Err(TbxError::DivisionByZero),
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x / y)),
        (Cell::Int(_), Cell::Float(y)) if y == 0.0 => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 / y)),
        (Cell::Float(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x / y as f64)),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    }
    Ok(())
}

pub fn mod_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    match (a, b) {
        (Cell::Int(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Int(y)) => vm.push(Cell::Int(x % y)),
        _ => {
            return Err(TbxError::TypeError {
                expected: "Int",
                got: "non-Int",
            })
        }
    }
    Ok(())
}

/// EQ — equality comparison. Pushes Bool(true) if the two top values are equal.
pub fn eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a == b));
    Ok(())
}

/// NEQ — inequality comparison. Pushes Bool(true) if the two top values are not equal.
pub fn neq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a != b));
    Ok(())
}

/// LT — less than. Pushes Bool(true) if a < b (numeric only, with Int/Float promotion).
pub fn lt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x < y,
        (Cell::Float(x), Cell::Float(y)) => x < y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) < *y,
        (Cell::Float(x), Cell::Int(y)) => *x < (*y as f64),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    };
    vm.push(Cell::Bool(result));
    Ok(())
}

/// GT — greater than. Pushes Bool(true) if a > b (numeric only, with Int/Float promotion).
pub fn gt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x > y,
        (Cell::Float(x), Cell::Float(y)) => x > y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) > *y,
        (Cell::Float(x), Cell::Int(y)) => *x > (*y as f64),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    };
    vm.push(Cell::Bool(result));
    Ok(())
}

/// LE — less than or equal. Pushes Bool(true) if a <= b (numeric only, with Int/Float promotion).
pub fn le_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x <= y,
        (Cell::Float(x), Cell::Float(y)) => x <= y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) <= *y,
        (Cell::Float(x), Cell::Int(y)) => *x <= (*y as f64),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    };
    vm.push(Cell::Bool(result));
    Ok(())
}

/// GE — greater than or equal. Pushes Bool(true) if a >= b (numeric only, with Int/Float promotion).
pub fn ge_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x >= y,
        (Cell::Float(x), Cell::Float(y)) => x >= y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) >= *y,
        (Cell::Float(x), Cell::Int(y)) => *x >= (*y as f64),
        _ => {
            return Err(TbxError::TypeError {
                expected: "number",
                got: "non-number",
            })
        }
    };
    vm.push(Cell::Bool(result));
    Ok(())
}

/// AND — logical AND. Evaluates both operands with is_truthy() and pushes the result as Bool.
pub fn and_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a.is_truthy() && b.is_truthy()));
    Ok(())
}

/// OR — logical OR. Evaluates both operands with is_truthy() and pushes the result as Bool.
pub fn or_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a.is_truthy() || b.is_truthy()));
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
    vm.register(WordEntry::new_primitive("ADD", add_prim));
    vm.register(WordEntry::new_primitive("SUB", sub_prim));
    vm.register(WordEntry::new_primitive("MUL", mul_prim));
    vm.register(WordEntry::new_primitive("DIV", div_prim));
    vm.register(WordEntry::new_primitive("MOD", mod_prim));
    vm.register(WordEntry::new_primitive("EQ", eq_prim));
    vm.register(WordEntry::new_primitive("NEQ", neq_prim));
    vm.register(WordEntry::new_primitive("LT", lt_prim));
    vm.register(WordEntry::new_primitive("GT", gt_prim));
    vm.register(WordEntry::new_primitive("LE", le_prim));
    vm.register(WordEntry::new_primitive("GE", ge_prim));
    vm.register(WordEntry::new_primitive("AND", and_prim));
    vm.register(WordEntry::new_primitive("OR", or_prim));
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
        assert_eq!(
            call_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Xt",
                got: "Int"
            })
        );
    }

    #[test]
    fn test_call_non_word_xt() {
        let mut vm = VM::new();
        register_all(&mut vm);
        let drop_xt = vm.lookup("DROP").unwrap();
        vm.dictionary.push(Cell::None);
        vm.dictionary.push(Cell::Xt(drop_xt)); // Xt exists but points to a Primitive
        vm.pc = 0;
        let original_bp = vm.bp;
        let original_rs_len = vm.return_stack.len();
        assert!(call_prim(&mut vm).is_err());

        // Verify that VM state remains unchanged after error
        assert_eq!(vm.pc, 0);
        assert_eq!(vm.bp, original_bp);
        assert_eq!(vm.return_stack.len(), original_rs_len);
    }

    #[test]
    fn test_add_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2));
        vm.push(Cell::Int(3));
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(5)));
    }

    #[test]
    fn test_add_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5));
        vm.push(Cell::Float(3.5));
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(6.0)));
    }

    #[test]
    fn test_add_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2));
        vm.push(Cell::Float(3.5));
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(5.5)));
    }

    #[test]
    fn test_add_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5));
        vm.push(Cell::Int(3));
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(5.5)));
    }

    #[test]
    fn test_add_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2));
        vm.push(Cell::Bool(true)); // Not a number
        assert_eq!(
            add_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "number",
                got: "non-number"
            })
        );
    }

    // --- sub_prim ---

    #[test]
    fn test_sub_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10));
        vm.push(Cell::Int(3));
        sub_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(7)));
    }

    #[test]
    fn test_sub_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(5.5));
        vm.push(Cell::Float(2.0));
        sub_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_sub_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5));
        vm.push(Cell::Float(1.5));
        sub_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_sub_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false));
        vm.push(Cell::Int(1));
        assert!(matches!(sub_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    // --- mul_prim ---

    #[test]
    fn test_mul_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(4));
        vm.push(Cell::Int(5));
        mul_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_mul_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5));
        vm.push(Cell::Float(4.0));
        mul_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(10.0)));
    }

    #[test]
    fn test_mul_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5));
        vm.push(Cell::Int(4));
        mul_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(10.0)));
    }

    #[test]
    fn test_mul_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Bool(true));
        assert!(matches!(mul_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    // --- div_prim ---

    #[test]
    fn test_div_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10));
        vm.push(Cell::Int(3));
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(3))); // truncation toward zero
    }

    #[test]
    fn test_div_int_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7));
        vm.push(Cell::Int(2));
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-3))); // truncation toward zero
    }

    #[test]
    fn test_div_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(7.0));
        vm.push(Cell::Float(2.0));
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_div_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7));
        vm.push(Cell::Float(2.0));
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_div_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(7.0));
        vm.push(Cell::Int(2));
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_div_by_zero_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5));
        vm.push(Cell::Int(0));
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_by_zero_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(5.0));
        vm.push(Cell::Float(0.0));
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_by_zero_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5));
        vm.push(Cell::Float(0.0));
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_by_zero_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(5.0));
        vm.push(Cell::Int(0));
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Int(1));
        assert!(matches!(div_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    // --- mod_prim ---

    #[test]
    fn test_mod_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7));
        vm.push(Cell::Int(3));
        mod_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_mod_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7));
        vm.push(Cell::Int(2));
        mod_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-1))); // truncation toward zero
    }

    #[test]
    fn test_mod_by_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5));
        vm.push(Cell::Int(0));
        assert_eq!(mod_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_mod_float_rejected() {
        let mut vm = VM::new();
        vm.push(Cell::Float(7.0));
        vm.push(Cell::Float(3.0));
        assert!(matches!(mod_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_mod_int_float_rejected() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7));
        vm.push(Cell::Float(3.0));
        assert!(matches!(mod_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    // --- EQ / NEQ tests ---

    #[test]
    fn test_eq_int_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        vm.push(Cell::Int(42));
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_int_not_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_eq_different_types() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Bool(true));
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_neq_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_neq_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5));
        vm.push(Cell::Int(5));
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    // --- LT / GT / LE / GE tests ---

    #[test]
    fn test_lt_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        lt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_lt_int_false() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3));
        vm.push(Cell::Int(2));
        lt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_lt_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.5));
        vm.push(Cell::Int(2));
        lt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_lt_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Int(1));
        assert!(matches!(lt_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_gt_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3));
        vm.push(Cell::Int(2));
        gt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_gt_int_false() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        gt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_le_int_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2));
        vm.push(Cell::Int(2));
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_le_int_less() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_le_int_greater() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3));
        vm.push(Cell::Int(2));
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_ge_int_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2));
        vm.push(Cell::Int(2));
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_ge_int_greater() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3));
        vm.push(Cell::Int(2));
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_ge_int_less() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    // --- AND / OR tests ---

    #[test]
    fn test_and_true_true() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Bool(true));
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_and_true_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Bool(false));
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_and_int_truthy() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(2));
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_and_int_zero_falsy() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Int(0));
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_or_false_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false));
        vm.push(Cell::Bool(false));
        or_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_or_true_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Bool(false));
        or_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_or_int_zero_and_nonzero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0));
        vm.push(Cell::Int(5));
        or_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }
}
