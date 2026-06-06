use std::collections::HashMap;

use tbx::tbx16::address::Address;
use tbx::tbx16::cell::Cell;
use tbx::tbx16::error::Tbx16Error;
use tbx::tbx16::memory::MEMORY_SIZE;
use tbx::tbx16::stack::{ReturnFrame, StackRegion};
use tbx::tbx16::{
    ExecutionOutcome, PrimitiveId, Tbx16Vm, CODE_TOKEN_DOCOL, CODE_TOKEN_PRIMITIVE, DATA_STACK_END,
    DATA_STACK_START, DEFAULT_RETURN_STACK_END, DEFAULT_RETURN_STACK_START,
};

const CODE_START: u16 = 0x0400;

#[derive(Debug, Clone, PartialEq, Eq)]
struct VmSnapshot {
    registers_ip: Option<Address>,
    dsp: Address,
    rsp: Address,
    bp: Address,
    step_counter: usize,
    call_depth: u16,
    memory: Vec<u8>,
}

fn snapshot(vm: &Tbx16Vm) -> VmSnapshot {
    VmSnapshot {
        registers_ip: vm.registers().ip,
        dsp: vm.registers().dsp,
        rsp: vm.registers().rsp,
        bp: vm.registers().bp,
        step_counter: vm.step_counter(),
        call_depth: vm.call_depth(),
        memory: vm.memory().as_bytes().to_vec(),
    }
}

enum PendingCell {
    Raw(Cell),
    Label(&'static str),
}

struct ImageBuilder {
    cursor: Address,
    cells: Vec<(Address, PendingCell)>,
    labels: HashMap<&'static str, Address>,
}

impl ImageBuilder {
    fn new(start: u16) -> Self {
        Self {
            cursor: Address::new(start),
            cells: Vec::new(),
            labels: HashMap::new(),
        }
    }

    fn primitive(&mut self, primitive: PrimitiveId) -> Cell {
        let xt = Cell::new(self.cursor.get());
        self.emit_cell(CODE_TOKEN_PRIMITIVE);
        self.emit_cell(primitive.as_cell());
        xt
    }

    fn colon_word(&mut self, arity: u16, locals: u16) -> Cell {
        let xt = Cell::new(self.cursor.get());
        self.emit_cell(CODE_TOKEN_DOCOL);
        self.emit_cell(Cell::new(arity));
        self.emit_cell(Cell::new(locals));
        xt
    }

    fn emit_xt(&mut self, xt: Cell) {
        self.emit_cell(xt);
    }

    fn emit_cell(&mut self, cell: Cell) {
        let addr = self.cursor;
        self.cells.push((addr, PendingCell::Raw(cell)));
        self.cursor = self
            .cursor
            .checked_add(2)
            .expect("test image stays within 64 KiB");
    }

    fn emit_label_ref(&mut self, label: &'static str) {
        let addr = self.cursor;
        self.cells.push((addr, PendingCell::Label(label)));
        self.cursor = self
            .cursor
            .checked_add(2)
            .expect("test image stays within 64 KiB");
    }

    fn mark_label(&mut self, label: &'static str) {
        self.labels.insert(label, self.cursor);
    }

    fn load_into(self, vm: &mut Tbx16Vm) {
        for (addr, pending) in self.cells {
            let cell = match pending {
                PendingCell::Raw(cell) => cell,
                PendingCell::Label(label) => {
                    let target = self.labels.get(label).expect("label must be defined");
                    Cell::new(target.get())
                }
            };
            vm.memory_mut().write_cell(addr, cell).unwrap();
        }
    }
}

#[test]
fn cell_reinterprets_u16_and_i16_bits() {
    let cell = Cell::from_i16(-2);
    assert_eq!(cell.raw(), 0xfffe);
    assert_eq!(cell.as_i16(), -2);
}

#[test]
fn cell_normalizes_truthy_values_to_canonical_booleans() {
    assert_eq!(Cell::new(0x0000).to_canonical_bool(), Cell::FALSE);
    assert_eq!(Cell::new(0x0001).to_canonical_bool(), Cell::TRUE);
    assert_eq!(Cell::new(0x1234).to_canonical_bool(), Cell::TRUE);
}

#[test]
fn memory_reads_and_writes_little_endian_cells() {
    let mut vm = Tbx16Vm::default();
    vm.memory_mut()
        .write_cell(Address::new(0x0200), Cell::new(0x1234))
        .unwrap();
    assert_eq!(vm.memory().as_bytes()[0x0200], 0x34);
    assert_eq!(vm.memory().as_bytes()[0x0201], 0x12);
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0200)).unwrap(),
        Cell::new(0x1234)
    );
}

#[test]
fn memory_allows_unaligned_cell_access() {
    let mut vm = Tbx16Vm::default();
    vm.memory_mut()
        .write_cell(Address::new(0x0201), Cell::new(0xabcd))
        .unwrap();
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0201)).unwrap(),
        Cell::new(0xabcd)
    );
}

#[test]
fn memory_rejects_cell_access_from_ffff() {
    let mut vm = Tbx16Vm::default();
    let write_err = vm
        .memory_mut()
        .write_cell(Address::new(0xffff), Cell::new(1))
        .unwrap_err();
    assert_eq!(
        write_err,
        Tbx16Error::InvalidMemoryAccess {
            addr: Address::new(0xffff),
            operation: "cell write",
        }
    );

    let read_err = vm.memory().read_cell(Address::new(0xffff)).unwrap_err();
    assert_eq!(
        read_err,
        Tbx16Error::InvalidMemoryAccess {
            addr: Address::new(0xffff),
            operation: "cell read",
        }
    );
}

#[test]
fn memory_starts_fully_zeroed() {
    let vm = Tbx16Vm::default();
    assert_eq!(vm.memory().as_bytes().len(), MEMORY_SIZE);
    assert!(vm.memory().as_bytes().iter().all(|byte| *byte == 0));
}

#[test]
fn memory_load_bytes_allows_single_byte_at_ffff() {
    let mut vm = Tbx16Vm::default();
    vm.memory_mut()
        .load_bytes(Address::new(0xffff), &[0xaa])
        .unwrap();
    assert_eq!(vm.memory().read_byte(Address::new(0xffff)).unwrap(), 0xaa);
}

#[test]
fn memory_zero_range_allows_single_byte_at_ffff() {
    let mut vm = Tbx16Vm::default();
    vm.memory_mut()
        .write_byte(Address::new(0xffff), 0xaa)
        .unwrap();
    vm.memory_mut().zero_range(Address::new(0xffff), 1).unwrap();
    assert_eq!(vm.memory().read_byte(Address::new(0xffff)).unwrap(), 0x00);
}

#[test]
fn memory_load_bytes_allows_full_64k_image() {
    let mut vm = Tbx16Vm::default();
    let image = vec![0x5a; MEMORY_SIZE];
    vm.memory_mut()
        .load_bytes(Address::new(0x0000), &image)
        .unwrap();
    assert!(vm.memory().as_bytes().iter().all(|byte| *byte == 0x5a));
}

#[test]
fn data_stack_starts_at_0080() {
    let vm = Tbx16Vm::default();
    assert_eq!(vm.registers().dsp, DATA_STACK_START);
    assert_eq!(vm.registers().bp, DATA_STACK_START);
}

#[test]
fn data_stack_push_and_pop_moves_dsp_by_two_bytes() {
    let mut vm = Tbx16Vm::default();
    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    assert_eq!(vm.registers().dsp, Address::new(0x0082));
    vm.push_data_cell(Cell::new(0x2222)).unwrap();
    assert_eq!(vm.registers().dsp, Address::new(0x0084));

    assert_eq!(vm.pop_data_cell().unwrap(), Cell::new(0x2222));
    assert_eq!(vm.registers().dsp, Address::new(0x0082));
    assert_eq!(vm.pop_data_cell().unwrap(), Cell::new(0x1111));
    assert_eq!(vm.registers().dsp, DATA_STACK_START);
}

#[test]
fn data_stack_reaches_0100_after_64_pushes_and_then_overflows() {
    let mut vm = Tbx16Vm::default();
    for i in 0..64u16 {
        vm.push_data_cell(Cell::new(i)).unwrap();
    }
    assert_eq!(vm.registers().dsp, DATA_STACK_END);
    assert_eq!(
        vm.push_data_cell(Cell::new(65)).unwrap_err(),
        Tbx16Error::DataStackOverflow
    );
}

#[test]
fn data_stack_underflow_is_reported_for_pop_and_peek() {
    let mut vm = Tbx16Vm::default();
    assert_eq!(
        vm.pop_data_cell().unwrap_err(),
        Tbx16Error::DataStackUnderflow
    );
    assert_eq!(
        vm.peek_data_cell(0).unwrap_err(),
        Tbx16Error::DataStackUnderflow
    );
}

#[test]
fn data_stack_values_are_written_into_fixed_memory_region_in_push_order() {
    let mut vm = Tbx16Vm::default();
    vm.push_data_cell(Cell::new(0x1001)).unwrap();
    vm.push_data_cell(Cell::new(0x1002)).unwrap();
    vm.push_data_cell(Cell::new(0x1003)).unwrap();

    assert_eq!(
        vm.memory().read_cell(Address::new(0x0080)).unwrap(),
        Cell::new(0x1001)
    );
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0082)).unwrap(),
        Cell::new(0x1002)
    );
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0084)).unwrap(),
        Cell::new(0x1003)
    );
}

#[test]
fn return_stack_starts_at_configured_region_start() {
    let vm = Tbx16Vm::default();
    assert_eq!(vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(
        vm.return_stack_region(),
        StackRegion::new(DEFAULT_RETURN_STACK_START, DEFAULT_RETURN_STACK_END).unwrap()
    );
}

#[test]
fn return_stack_push_and_pop_moves_rsp_by_two_bytes() {
    let mut vm = Tbx16Vm::default();
    vm.push_return_cell(Cell::new(0x2001)).unwrap();
    assert_eq!(vm.registers().rsp, Address::new(0x0202));
    vm.push_return_cell(Cell::new(0x2002)).unwrap();
    assert_eq!(vm.registers().rsp, Address::new(0x0204));

    assert_eq!(vm.pop_return_cell().unwrap(), Cell::new(0x2002));
    assert_eq!(vm.registers().rsp, Address::new(0x0202));
    assert_eq!(vm.pop_return_cell().unwrap(), Cell::new(0x2001));
    assert_eq!(vm.registers().rsp, DEFAULT_RETURN_STACK_START);
}

#[test]
fn return_frame_is_stored_as_two_cells_in_memory() {
    let mut vm = Tbx16Vm::default();
    let frame = ReturnFrame {
        return_ip: Address::new(0x3456),
        caller_bp: Address::new(0x00a0),
    };

    vm.push_return_frame(frame).unwrap();
    assert_eq!(vm.registers().rsp, Address::new(0x0204));
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0200)).unwrap(),
        Cell::new(0x3456)
    );
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0202)).unwrap(),
        Cell::new(0x00a0)
    );
    assert_eq!(vm.peek_return_cell(0).unwrap(), Cell::new(0x00a0));
}

#[test]
fn return_frame_round_trips_return_ip_and_caller_bp() {
    let mut vm = Tbx16Vm::default();
    let frame = ReturnFrame {
        return_ip: Address::new(0x4567),
        caller_bp: Address::new(0x00c0),
    };

    vm.push_return_frame(frame).unwrap();
    assert_eq!(vm.pop_return_frame().unwrap(), frame);
    assert_eq!(vm.registers().rsp, DEFAULT_RETURN_STACK_START);
}

#[test]
fn return_stack_reports_overflow_and_underflow() {
    let region = StackRegion::new(Address::new(0x0200), Address::new(0x0204)).unwrap();
    let mut vm = Tbx16Vm::new(region).unwrap();

    vm.push_return_cell(Cell::new(1)).unwrap();
    vm.push_return_cell(Cell::new(2)).unwrap();
    assert_eq!(
        vm.push_return_cell(Cell::new(3)).unwrap_err(),
        Tbx16Error::ReturnStackOverflow
    );

    let mut empty_vm = Tbx16Vm::new(region).unwrap();
    assert_eq!(
        empty_vm.pop_return_cell().unwrap_err(),
        Tbx16Error::ReturnStackUnderflow
    );
    assert_eq!(
        empty_vm.pop_return_frame().unwrap_err(),
        Tbx16Error::ReturnStackUnderflow
    );
}

#[test]
fn invalid_return_stack_regions_are_rejected() {
    let page_one_overlap = StackRegion::new(Address::new(0x0180), Address::new(0x0280)).unwrap();
    let err = Tbx16Vm::new(page_one_overlap).unwrap_err();
    assert!(matches!(err, Tbx16Error::InvalidStackRegion { .. }));

    let data_stack_overlap = StackRegion::new(Address::new(0x00f0), Address::new(0x0120)).unwrap();
    let err = Tbx16Vm::new(data_stack_overlap).unwrap_err();
    assert!(matches!(err, Tbx16Error::InvalidStackRegion { .. }));
}

#[test]
fn return_stack_capacity_matches_pushable_cell_count() {
    let region = StackRegion::new(Address::new(0x0200), Address::new(0x0208)).unwrap();
    let vm = Tbx16Vm::new(region).unwrap();
    assert_eq!(usize::from(region.len_bytes()) / 2, 4);
    assert_eq!(vm.return_stack_region(), region);
}

#[test]
fn return_stack_full_region_ends_with_rsp_at_region_end() {
    let region = StackRegion::new(Address::new(0x0200), Address::new(0x0208)).unwrap();
    let mut vm = Tbx16Vm::new(region).unwrap();

    for i in 0..(usize::from(region.len_bytes()) / 2) {
        vm.push_return_cell(Cell::new(i as u16)).unwrap();
    }

    assert_eq!(vm.registers().rsp, region.end());
    assert_eq!(
        vm.push_return_cell(Cell::new(0xffff)).unwrap_err(),
        Tbx16Error::ReturnStackOverflow
    );
}

#[test]
fn stack_state_is_observed_through_memory_not_shadow_containers() {
    let mut vm = Tbx16Vm::default();
    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    vm.push_data_cell(Cell::new(0x2222)).unwrap();
    vm.memory_mut()
        .write_cell(Address::new(0x0082), Cell::new(0x9999))
        .unwrap();
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x9999));
}

#[test]
fn bp_slot_addresses_use_two_byte_steps() {
    let vm = Tbx16Vm::default();
    assert_eq!(vm.data_slot_address(0).unwrap(), Address::new(0x0080));
    assert_eq!(vm.data_slot_address(1).unwrap(), Address::new(0x0082));
    assert_eq!(vm.data_slot_address(5).unwrap(), Address::new(0x008a));
}

#[test]
fn primitive_and_colon_entries_share_the_same_xt_namespace() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    let exit_xt = image.primitive(PrimitiveId::Exit);
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(0));
    image.load_into(&mut vm);

    assert_eq!(halt_xt.raw() % 2, 0);
    assert_eq!(entry_xt.raw() % 2, 0);
    assert_eq!(vm.run(halt_xt), ExecutionOutcome::Halted);
    assert_eq!(vm.run(entry_xt), ExecutionOutcome::Returned);
}

#[test]
fn invalid_xts_and_malformed_word_layouts_trap() {
    let mut vm = Tbx16Vm::default();
    vm.memory_mut()
        .write_cell(Address::new(CODE_START), Cell::new(0x9999))
        .unwrap();
    vm.memory_mut()
        .write_cell(Address::new(0xfffe), CODE_TOKEN_PRIMITIVE)
        .unwrap();
    vm.memory_mut()
        .write_cell(Address::new(0xfffc), CODE_TOKEN_DOCOL)
        .unwrap();
    vm.memory_mut()
        .write_cell(Address::new(0xfffe), Cell::new(1))
        .unwrap();

    assert_eq!(
        vm.run(Cell::new(CODE_START)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(CODE_START),
        })
    );
    assert_eq!(
        vm.run(Cell::new(CODE_START + 1)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(CODE_START + 1),
        })
    );
    assert_eq!(
        vm.run(Cell::new(0xffff)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(0xffff),
        })
    );
    assert_eq!(
        vm.run(Cell::new(0xfffe)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(0xfffe),
        })
    );
    assert_eq!(
        vm.run(Cell::new(0xfffc)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(0xfffc),
        })
    );
}

#[test]
fn entry_colon_initialization_validates_arity_and_zeroes_locals() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    let entry_xt = image.colon_word(2, 2);
    image.emit_xt(halt_xt);
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    vm.push_data_cell(Cell::new(0x2222)).unwrap();

    assert_eq!(vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(vm.registers().bp, Address::new(0x0080));
    assert_eq!(vm.registers().dsp, Address::new(0x0088));
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0084)).unwrap(),
        Cell::new(0)
    );
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0086)).unwrap(),
        Cell::new(0)
    );

    let mut arity_vm = Tbx16Vm::default();
    let mut arity_image = ImageBuilder::new(CODE_START);
    let bad_entry_xt = arity_image.colon_word(1, 0);
    arity_image.load_into(&mut arity_vm);
    assert_eq!(
        arity_vm.run(bad_entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
}

#[test]
fn lit_and_top_level_exit_one_cell_return_work() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let exit_xt = image.primitive(PrimitiveId::Exit);
    let entry_xt = image.colon_word(1, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x7777));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(0xaaaa)).unwrap();
    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    assert_eq!(vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.registers().bp, DATA_STACK_START);
    assert_eq!(vm.registers().dsp, Address::new(0x0084));
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x7777));
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0080)).unwrap(),
        Cell::new(0xaaaa)
    );
}

#[test]
fn branch_and_zbranch_follow_absolute_targets() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let branch_xt = image.primitive(PrimitiveId::Branch);
    let zbranch_xt = image.primitive(PrimitiveId::ZBranch);
    let exit_xt = image.primitive(PrimitiveId::Exit);

    let branch_entry_xt = image.colon_word(0, 0);
    image.emit_xt(branch_xt);
    image.emit_label_ref("branch_target");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0xdead));
    image.mark_label("branch_target");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x1234));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let _zbranch_zero_xt = image.colon_word(0, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0));
    image.emit_xt(zbranch_xt);
    image.emit_label_ref("z_zero_target");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x9999));
    image.mark_label("z_zero_target");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x2222));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let _zbranch_nonzero_xt = image.colon_word(0, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(1));
    image.emit_xt(zbranch_xt);
    image.emit_label_ref("z_nonzero_target");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x3333));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));
    image.mark_label("z_nonzero_target");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x4444));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    image.load_into(&mut vm);

    assert_eq!(vm.run(branch_entry_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x1234));

    let mut zero_vm = Tbx16Vm::default();
    let mut zero_image = ImageBuilder::new(CODE_START);
    let lit_xt = zero_image.primitive(PrimitiveId::Lit);
    let zbranch_xt = zero_image.primitive(PrimitiveId::ZBranch);
    let exit_xt = zero_image.primitive(PrimitiveId::Exit);
    let entry_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(lit_xt);
    zero_image.emit_cell(Cell::new(0));
    zero_image.emit_xt(zbranch_xt);
    zero_image.emit_label_ref("target");
    zero_image.emit_xt(lit_xt);
    zero_image.emit_cell(Cell::new(0x9999));
    zero_image.mark_label("target");
    zero_image.emit_xt(lit_xt);
    zero_image.emit_cell(Cell::new(0x2222));
    zero_image.emit_xt(exit_xt);
    zero_image.emit_cell(Cell::new(1));
    zero_image.load_into(&mut zero_vm);
    assert_eq!(zero_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(zero_vm.peek_data_cell(0).unwrap(), Cell::new(0x2222));

    let mut nonzero_vm = Tbx16Vm::default();
    let mut nonzero_image = ImageBuilder::new(CODE_START);
    let lit_xt = nonzero_image.primitive(PrimitiveId::Lit);
    let zbranch_xt = nonzero_image.primitive(PrimitiveId::ZBranch);
    let exit_xt = nonzero_image.primitive(PrimitiveId::Exit);
    let entry_xt = nonzero_image.colon_word(0, 0);
    nonzero_image.emit_xt(lit_xt);
    nonzero_image.emit_cell(Cell::new(1));
    nonzero_image.emit_xt(zbranch_xt);
    nonzero_image.emit_label_ref("target");
    nonzero_image.emit_xt(lit_xt);
    nonzero_image.emit_cell(Cell::new(0x3333));
    nonzero_image.emit_xt(exit_xt);
    nonzero_image.emit_cell(Cell::new(1));
    nonzero_image.mark_label("target");
    nonzero_image.emit_xt(lit_xt);
    nonzero_image.emit_cell(Cell::new(0x4444));
    nonzero_image.emit_xt(exit_xt);
    nonzero_image.emit_cell(Cell::new(1));
    nonzero_image.load_into(&mut nonzero_vm);
    assert_eq!(nonzero_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(nonzero_vm.peek_data_cell(0).unwrap(), Cell::new(0x3333));
}

#[test]
fn invalid_branch_targets_and_return_counts_trap() {
    let mut odd_vm = Tbx16Vm::default();
    let mut odd_image = ImageBuilder::new(CODE_START);
    let branch_xt = odd_image.primitive(PrimitiveId::Branch);
    let entry_xt = odd_image.colon_word(0, 0);
    odd_image.emit_xt(branch_xt);
    odd_image.emit_cell(Cell::new(0x0401));
    odd_image.load_into(&mut odd_vm);
    assert_eq!(
        odd_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0x0401),
        })
    );

    let mut ffff_vm = Tbx16Vm::default();
    let mut ffff_image = ImageBuilder::new(CODE_START);
    let branch_xt = ffff_image.primitive(PrimitiveId::Branch);
    let entry_xt = ffff_image.colon_word(0, 0);
    ffff_image.emit_xt(branch_xt);
    ffff_image.emit_cell(Cell::new(0xffff));
    ffff_image.load_into(&mut ffff_vm);
    assert_eq!(
        ffff_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0xffff),
        })
    );

    let mut return_vm = Tbx16Vm::default();
    let mut return_image = ImageBuilder::new(CODE_START);
    let exit_xt = return_image.primitive(PrimitiveId::Exit);
    let entry_xt = return_image.colon_word(0, 0);
    return_image.emit_xt(exit_xt);
    return_image.emit_cell(Cell::new(2));
    return_image.load_into(&mut return_vm);
    assert_eq!(
        return_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidReturnCount {
            count: Cell::new(2),
        })
    );
}

#[test]
fn nested_calls_restore_frame_state_and_return_values() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let exit_xt = image.primitive(PrimitiveId::Exit);
    let halt_xt = image.primitive(PrimitiveId::Halt);

    let deep_xt = image.colon_word(0, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x7777));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let mid_xt = image.colon_word(1, 0);
    image.emit_xt(deep_xt);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let top_xt = image.colon_word(1, 0);
    image.emit_xt(mid_xt);
    image.emit_xt(halt_xt);
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(0xaaaa)).unwrap();
    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    assert_eq!(vm.run(top_xt), ExecutionOutcome::Halted);
    assert_eq!(vm.call_depth(), 0);
    assert_eq!(vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(vm.registers().bp, Address::new(0x0082));
    assert_eq!(vm.registers().dsp, Address::new(0x0084));
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x7777));
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0080)).unwrap(),
        Cell::new(0xaaaa)
    );
}

#[test]
fn step_limit_counts_entry_resolution_and_stops_before_next_dispatch() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let exit_xt = image.primitive(PrimitiveId::Exit);
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(0));
    image.load_into(&mut vm);

    vm.set_step_limit(Some(1));
    assert_eq!(
        vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::StepLimitExceeded)
    );
    assert_eq!(vm.step_counter(), 1);
    assert_eq!(vm.registers().ip, Some(Address::new(CODE_START + 10)));

    let mut ok_vm = Tbx16Vm::default();
    let mut ok_image = ImageBuilder::new(CODE_START);
    let exit_xt = ok_image.primitive(PrimitiveId::Exit);
    let entry_xt = ok_image.colon_word(0, 0);
    ok_image.emit_xt(exit_xt);
    ok_image.emit_cell(Cell::new(0));
    ok_image.load_into(&mut ok_vm);
    ok_vm.set_step_limit(Some(2));
    assert_eq!(ok_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(ok_vm.step_counter(), 2);
}

#[test]
fn lit_push_failure_is_atomic() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x1111));
    image.load_into(&mut vm);

    for i in 0..64u16 {
        vm.push_data_cell(Cell::new(i)).unwrap();
    }
    assert_eq!(
        vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackOverflow)
    );
    let after = snapshot(&vm);
    assert_eq!(after.dsp, DATA_STACK_END);
    assert_eq!(after.rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(after.bp, DATA_STACK_END);
    assert_eq!(after.registers_ip, Some(Address::new(CODE_START + 10)));
    assert_eq!(
        vm.memory().read_cell(Address::new(0x00fe)).unwrap(),
        Cell::new(63)
    );
}

#[test]
fn zbranch_underflow_is_atomic() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let zbranch_xt = image.primitive(PrimitiveId::ZBranch);
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(zbranch_xt);
    image.emit_label_ref("target");
    image.mark_label("target");
    image.load_into(&mut vm);

    assert_eq!(
        vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    assert_eq!(vm.registers().ip, Some(Address::new(CODE_START + 10)));
    assert_eq!(vm.registers().dsp, DATA_STACK_START);
}

#[test]
fn call_and_return_failures_leave_current_frame_state_unchanged() {
    let mut call_vm = Tbx16Vm::default();
    let mut call_image = ImageBuilder::new(CODE_START);
    let bad_callee_xt = call_image.colon_word(1, 0);
    let entry_xt = call_image.colon_word(0, 0);
    call_image.emit_xt(bad_callee_xt);
    call_image.load_into(&mut call_vm);
    assert_eq!(
        call_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    assert_eq!(call_vm.call_depth(), 0);
    assert_eq!(call_vm.registers().bp, DATA_STACK_START);
    assert_eq!(call_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(call_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(call_vm.registers().ip, Some(Address::new(CODE_START + 12)));

    let mut ret_vm = Tbx16Vm::default();
    let mut ret_image = ImageBuilder::new(CODE_START);
    let exit_xt = ret_image.primitive(PrimitiveId::Exit);
    let callee_xt = ret_image.colon_word(0, 0);
    ret_image.emit_xt(exit_xt);
    ret_image.emit_cell(Cell::new(1));
    let ret_entry_xt = ret_image.colon_word(0, 0);
    ret_image.emit_xt(callee_xt);
    ret_image.load_into(&mut ret_vm);
    assert_eq!(
        ret_vm.run(ret_entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    assert_eq!(ret_vm.call_depth(), 1);
    assert_eq!(ret_vm.registers().rsp, Address::new(0x0204));
    assert_eq!(ret_vm.registers().ip, Some(Address::new(CODE_START + 10)));
}
