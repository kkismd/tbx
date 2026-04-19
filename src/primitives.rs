use crate::cell::Cell;
use crate::constants::MAX_DICTIONARY_CELLS;
use crate::dict::{EntryKind, WordEntry};
use crate::error::TbxError;
use crate::vm::VM;

/// DROP — discard the top element of the data stack.
pub fn drop_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.pop()?;
    Ok(())
}

/// LIT_MARKER — push a Cell::Marker sentinel onto the data stack.
pub fn lit_marker_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::Marker);
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
/// Int/Float mixed pairs are compared by promoting Int to Float.
pub fn eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) == *y,
        (Cell::Float(x), Cell::Int(y)) => *x == (*y as f64),
        _ => a == b,
    };
    vm.push(Cell::Bool(result));
    Ok(())
}

/// NEQ — inequality comparison. Pushes Bool(true) if the two top values are not equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
pub fn neq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) != *y,
        (Cell::Float(x), Cell::Int(y)) => *x != (*y as f64),
        _ => a != b,
    };
    vm.push(Cell::Bool(result));
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

/// PUTSTR — output the string referenced by a StringDesc on the stack (no newline).
/// Escape sequences (\n, \t, \\) in the stored string are output literally
/// as they were already expanded at compile time (during intern).
pub fn putstr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    match cell {
        Cell::StringDesc(idx) => {
            let s = vm.resolve_string(idx)?;
            vm.write_output(&s);
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "StringDesc",
            got: cell.type_name(),
        }),
    }
}

/// PUTCHR — output the integer value on the stack as a single ASCII character (no newline).
pub fn putchr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    match cell {
        Cell::Int(code) => {
            if !(0..=127).contains(&code) {
                return Err(TbxError::TypeError {
                    expected: "ASCII code (0-127)",
                    got: "out of range",
                });
            }
            let ch = code as u8 as char;
            vm.write_output(&ch.to_string());
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "Int",
            got: cell.type_name(),
        }),
    }
}

/// PUTDEC — output the integer value on the stack as a signed decimal number (no newline).
pub fn putdec_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    match cell {
        Cell::Int(n) => {
            vm.write_output(&n.to_string());
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "Int",
            got: cell.type_name(),
        }),
    }
}

/// PUTHEX — output the integer value on the stack as $-prefixed uppercase hex (no newline).
/// Negative values are output as two's complement 64-bit representation.
pub fn puthex_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    match cell {
        Cell::Int(n) => {
            if n < 0 {
                vm.write_output(&format!("${:X}", n as u64));
            } else {
                vm.write_output(&format!("${:X}", n));
            }
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "Int",
            got: cell.type_name(),
        }),
    }
}

/// APPEND — pop a Cell and write it to dictionary[dp], advancing dp by 1.
pub fn append_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    vm.dict_write(cell)
}

/// ALLOT — pop N from the stack, advance dp by N cells, and push the start address.
pub fn allot_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    let n = match cell {
        Cell::Int(n) => n,
        _ => {
            return Err(TbxError::TypeError {
                expected: "Int",
                got: cell.type_name(),
            })
        }
    };
    if n < 0 {
        return Err(TbxError::InvalidAllotCount);
    }
    let count = n as usize;
    let new_dp = vm.dp + count;
    if new_dp > MAX_DICTIONARY_CELLS {
        return Err(TbxError::DictionaryOverflow {
            requested: new_dp,
            limit: MAX_DICTIONARY_CELLS,
        });
    }
    let start = vm.dp;
    for _ in 0..count {
        vm.dict_write(Cell::None)?;
    }
    vm.push(Cell::DictAddr(start));
    Ok(())
}

/// HERE — push the current dictionary pointer as a DictAddr.
pub fn here_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::DictAddr(vm.dp));
    Ok(())
}

/// STATE — push the current compile mode flag as an Int (0 = execute, 1 = compile).
pub fn state_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::Int(if vm.is_compiling { 1 } else { 0 }));
    Ok(())
}

/// HALT — stop VM execution by returning a Halted error.
pub fn halt_prim(_vm: &mut VM) -> Result<(), TbxError> {
    Err(TbxError::Halted)
}

/// LITERAL — compile a literal value into the dictionary as LIT + value (2 cells).
pub fn literal_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    let lit_xt = vm.lookup("LIT").ok_or(TbxError::TypeError {
        expected: "LIT word to be registered",
        got: "not found",
    })?;
    vm.dict_write(Cell::Xt(lit_xt))?;
    vm.dict_write(value)?;
    Ok(())
}
/// Register all stack primitives into the VM's dictionary.
pub fn register_all(vm: &mut VM) {
    vm.register(WordEntry::new_primitive("DROP", drop_prim));
    vm.register(WordEntry::new_primitive("DUP", dup_prim));
    vm.register(WordEntry::new_primitive("SWAP", swap_prim));
    vm.register(WordEntry::new_primitive("FETCH", fetch_prim));
    vm.register(WordEntry::new_primitive("STORE", store_prim));
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
    vm.register(WordEntry::new_primitive("PUTSTR", putstr_prim));
    vm.register(WordEntry::new_primitive("PUTCHR", putchr_prim));
    vm.register(WordEntry::new_primitive("PUTDEC", putdec_prim));
    vm.register(WordEntry::new_primitive("PUTHEX", puthex_prim));
    vm.register(WordEntry::new_primitive("APPEND", append_prim));
    vm.register(WordEntry::new_primitive("ALLOT", allot_prim));
    vm.register(WordEntry::new_primitive("HERE", here_prim));
    vm.register(WordEntry::new_primitive("STATE", state_prim));
    vm.register(WordEntry::new_primitive("HALT", halt_prim));
    vm.register(WordEntry {
        name: "CALL".to_string(),
        flags: 0,
        kind: EntryKind::Call,
        prev: None,
    });
    vm.register(WordEntry {
        name: "EXIT".to_string(),
        flags: 0,
        kind: EntryKind::Exit,
        prev: None,
    });
    vm.register(WordEntry {
        name: "RETURN_VAL".to_string(),
        flags: 0,
        kind: EntryKind::ReturnVal,
        prev: None,
    });
    vm.register(WordEntry {
        name: "DROP_TO_MARKER".to_string(),
        flags: 0,
        kind: EntryKind::DropToMarker,
        prev: None,
    });
    // TODO(#164): LIT_MARKER and DROP_TO_MARKER are registered with flags=0, allowing user code
    // to call them directly. Once a FLAG_SYSTEM mechanism is in place, protect these words.
    vm.register(WordEntry::new_primitive("LIT_MARKER", lit_marker_prim));
    vm.register(WordEntry {
        name: "LIT".to_string(),
        flags: 0,
        kind: EntryKind::Lit,
        prev: None,
    });
    let mut literal_entry = WordEntry::new_primitive("LITERAL", literal_prim);
    literal_entry.flags |= crate::dict::FLAG_IMMEDIATE;
    vm.register(literal_entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
    use crate::constants::MAX_DICTIONARY_CELLS;

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
    fn test_eq_int_float_promotion() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Float(1.0));
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_float_int_promotion() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.0));
        vm.push(Cell::Int(2));
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_int_float_not_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Float(1.5));
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_neq_int_float_promotion() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        vm.push(Cell::Float(1.0));
        neq_prim(&mut vm).unwrap();
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
    fn test_gt_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(3.5));
        vm.push(Cell::Int(2));
        gt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_gt_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Int(1));
        assert!(matches!(gt_prim(&mut vm), Err(TbxError::TypeError { .. })));
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
    fn test_le_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.5));
        vm.push(Cell::Int(2));
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_le_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Int(1));
        assert!(matches!(le_prim(&mut vm), Err(TbxError::TypeError { .. })));
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

    #[test]
    fn test_ge_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.0));
        vm.push(Cell::Int(2));
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_ge_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        vm.push(Cell::Int(1));
        assert!(matches!(ge_prim(&mut vm), Err(TbxError::TypeError { .. })));
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

    // --- PUTSTR tests ---

    #[test]
    fn test_putstr_basic() {
        let mut vm = VM::new();
        let idx = vm.intern_string("hello").unwrap();
        vm.push(Cell::StringDesc(idx));
        putstr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "hello");
    }

    #[test]
    fn test_putstr_empty() {
        let mut vm = VM::new();
        let idx = vm.intern_string("").unwrap();
        vm.push(Cell::StringDesc(idx));
        putstr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "");
    }

    #[test]
    fn test_putstr_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        assert!(matches!(
            putstr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putstr_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            putstr_prim(&mut vm),
            Err(TbxError::StackUnderflow)
        ));
    }

    // --- PUTCHR tests ---

    #[test]
    fn test_putchr_basic() {
        let mut vm = VM::new();
        vm.push(Cell::Int(65)); // 'A'
        putchr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "A");
    }

    #[test]
    fn test_putchr_newline() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)); // '\n'
        putchr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "\n");
    }

    #[test]
    fn test_putchr_out_of_range() {
        let mut vm = VM::new();
        vm.push(Cell::Int(128));
        assert!(matches!(
            putchr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putchr_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1));
        assert!(matches!(
            putchr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putchr_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        assert!(matches!(
            putchr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- PUTDEC tests ---

    #[test]
    fn test_putdec_positive() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "42");
    }

    #[test]
    fn test_putdec_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7));
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "-7");
    }

    #[test]
    fn test_putdec_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0));
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "0");
    }

    #[test]
    fn test_putdec_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Float(3.5));
        assert!(matches!(
            putdec_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- PUTHEX tests ---

    #[test]
    fn test_puthex_positive() {
        let mut vm = VM::new();
        vm.push(Cell::Int(255));
        puthex_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "$FF");
    }

    #[test]
    fn test_puthex_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0));
        puthex_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "$0");
    }

    #[test]
    fn test_puthex_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1));
        puthex_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "$FFFFFFFFFFFFFFFF");
    }

    #[test]
    fn test_puthex_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        assert!(matches!(
            puthex_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- append_prim ---

    #[test]
    fn test_append_writes_to_dictionary() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        append_prim(&mut vm).unwrap();
        assert_eq!(vm.dictionary.len(), 1);
        assert_eq!(vm.dp, 1);
        assert!(matches!(vm.dictionary[0], Cell::Int(42)));
    }

    #[test]
    fn test_append_xt_value() {
        let mut vm = VM::new();
        vm.push(Cell::Xt(crate::cell::Xt(5)));
        append_prim(&mut vm).unwrap();
        assert_eq!(vm.dictionary.len(), 1);
        assert!(matches!(vm.dictionary[0], Cell::Xt(_)));
    }

    #[test]
    fn test_append_multiple() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10));
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(20));
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(30));
        append_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 3);
        assert!(matches!(vm.dictionary[0], Cell::Int(10)));
        assert!(matches!(vm.dictionary[1], Cell::Int(20)));
        assert!(matches!(vm.dictionary[2], Cell::Int(30)));
    }

    #[test]
    fn test_append_empty_stack() {
        let mut vm = VM::new();
        assert!(matches!(
            append_prim(&mut vm),
            Err(TbxError::StackUnderflow)
        ));
    }

    #[test]
    fn test_append_overflow() {
        let mut vm = VM::new();
        vm.dp = MAX_DICTIONARY_CELLS;
        // Manually grow dictionary to match dp invariant
        vm.dictionary.resize(MAX_DICTIONARY_CELLS, Cell::None);
        vm.push(Cell::Int(1));
        assert!(matches!(
            append_prim(&mut vm),
            Err(TbxError::DictionaryOverflow { .. })
        ));
    }

    // --- allot_prim ---

    #[test]
    fn test_allot_reserves_cells() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5));
        allot_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 5);
        assert_eq!(vm.dictionary.len(), 5);
        // Returns start address
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(0));
    }

    #[test]
    fn test_allot_after_append() {
        let mut vm = VM::new();
        vm.push(Cell::Int(100));
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(3));
        allot_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 4);
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(1));
    }

    #[test]
    fn test_allot_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0));
        allot_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 0);
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(0));
    }

    #[test]
    fn test_allot_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1));
        assert!(matches!(
            allot_prim(&mut vm),
            Err(TbxError::InvalidAllotCount)
        ));
    }

    #[test]
    fn test_allot_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true));
        assert!(matches!(
            allot_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- here_prim ---

    #[test]
    fn test_here_initial() {
        let mut vm = VM::new();
        here_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(0));
    }

    #[test]
    fn test_here_after_append() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1));
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(2));
        append_prim(&mut vm).unwrap();
        here_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(2));
    }

    // --- dict_write overflow ---

    #[test]
    fn test_allot_overflow() {
        let mut vm = VM::new();
        vm.push(Cell::Int((MAX_DICTIONARY_CELLS + 1) as i64));
        assert!(matches!(
            allot_prim(&mut vm),
            Err(TbxError::DictionaryOverflow { .. })
        ));
    }

    // --- state_prim ---

    #[test]
    fn test_state_execute_mode() {
        let mut vm = VM::new();
        state_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(0));
    }

    #[test]
    fn test_state_compile_mode() {
        let mut vm = VM::new();
        vm.is_compiling = true;
        state_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(1));
    }

    // --- halt_prim ---

    #[test]
    fn test_halt_returns_halted() {
        let mut vm = VM::new();
        assert!(matches!(halt_prim(&mut vm), Err(TbxError::Halted)));
    }

    #[test]
    fn test_halt_leaves_stack_unchanged() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        let _ = halt_prim(&mut vm);
        assert_eq!(vm.data_stack.len(), 1);
        assert_eq!(vm.pop().unwrap(), Cell::Int(42));
    }

    #[test]
    fn test_literal_compiles_lit_and_value() {
        // LITERAL should write [Xt(LIT), value] into the dictionary.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_xt = vm.lookup("LIT").unwrap();
        let dp_before = vm.dp;

        vm.push(Cell::Int(123));
        crate::primitives::literal_prim(&mut vm).unwrap();

        assert_eq!(vm.dictionary[dp_before], Cell::Xt(lit_xt));
        assert_eq!(vm.dictionary[dp_before + 1], Cell::Int(123));
        assert_eq!(vm.dp, dp_before + 2);
    }
}
