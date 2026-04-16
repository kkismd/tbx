use crate::dict::WordEntry;
use crate::cell::Cell;

/// The TBX virtual machine.
///
/// Holds all runtime state: dictionary, string pool, stacks, and registers.
#[derive(Debug)]
pub struct VM {
    /// The dictionary: all registered words (system primitives + user-defined)
    pub dictionary: Vec<WordEntry>,
    /// String pool: all string data packed as length-prefixed byte sequences
    pub string_pool: Vec<u8>,
    /// Data stack: operand stack for arithmetic and parameter passing
    pub data_stack: Vec<Cell>,
    /// Return stack: saves (pc, bp) pairs on word calls
    pub return_stack: Vec<(usize, usize)>,
    /// Program counter: index of the currently executing word in the dictionary
    pub pc: usize,
    /// Base pointer: index into data_stack marking the current stack frame base
    pub bp: usize,
    /// End of system dictionary (primitives registered at startup)
    pub dp_sys: usize,
    /// End of standard library dictionary
    pub dp_lib: usize,
    /// End of user dictionary
    pub dp_user: usize,
}

impl VM {
    /// Create a new VM with empty dictionary and stacks.
    pub fn new() -> Self {
        Self {
            dictionary: Vec::new(),
            string_pool: Vec::new(),
            data_stack: Vec::new(),
            return_stack: Vec::new(),
            pc: 0,
            bp: 0,
            dp_sys: 0,
            dp_lib: 0,
            dp_user: 0,
        }
    }

    /// Look up a word by name, searching from newest to oldest entry.
    /// Returns the dictionary index (Xt) if found.
    pub fn lookup(&self, name: &str) -> Option<usize> {
        self.dictionary
            .iter()
            .enumerate()
            .rev()
            .find(|(_, entry)| entry.name == name)
            .map(|(idx, _)| idx)
    }

    /// Push a value onto the data stack.
    pub fn push(&mut self, cell: Cell) {
        self.data_stack.push(cell);
    }

    /// Pop a value from the data stack.
    pub fn pop(&mut self) -> Option<Cell> {
        self.data_stack.pop()
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_new() {
        let vm = VM::new();
        assert!(vm.dictionary.is_empty());
        assert!(vm.data_stack.is_empty());
        assert!(vm.return_stack.is_empty());
    }

    #[test]
    fn test_push_pop() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        assert_eq!(vm.pop(), Some(Cell::Int(42)));
        assert_eq!(vm.pop(), None);
    }

    #[test]
    fn test_lookup() {
        let mut vm = VM::new();
        use crate::dict::WordEntry;
        vm.dictionary.push(WordEntry::new_primitive("HALT"));
        vm.dictionary.push(WordEntry::new_primitive("DROP"));

        assert_eq!(vm.lookup("HALT"), Some(0));
        assert_eq!(vm.lookup("DROP"), Some(1));
        assert_eq!(vm.lookup("MISSING"), None);
    }
}
