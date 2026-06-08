use std::collections::HashMap;

use tbx::tbx16::address::Address;
use tbx::tbx16::cell::Cell;
use tbx::tbx16::error::Tbx16Error;
use tbx::tbx16::memory::MEMORY_SIZE;
use tbx::tbx16::stack::{ReturnFrame, StackRegion};
use tbx::tbx16::{
    primitive_descriptor_by_id, primitive_descriptor_by_name, ExecutionOutcome, PrimitiveId,
    PrimitiveOperand, ResolvedWord, Tbx16Vm, CODE_TOKEN_DOCOL, CODE_TOKEN_PRIMITIVE,
    DATA_STACK_END, DATA_STACK_START, DEFAULT_RETURN_STACK_END, DEFAULT_RETURN_STACK_START,
    PRIMITIVE_REGISTRY,
};

const CODE_START: u16 = 0x0400;

#[derive(Debug, Clone, PartialEq, Eq)]
struct VmSnapshot {
    ip: Option<Address>,
    dsp: Address,
    rsp: Address,
    bp: Address,
    step_counter: usize,
    call_depth: u16,
    output: Vec<u8>,
    memory: Vec<u8>,
}

fn snapshot(vm: &Tbx16Vm) -> VmSnapshot {
    VmSnapshot {
        ip: vm.registers().ip,
        dsp: vm.registers().dsp,
        rsp: vm.registers().rsp,
        bp: vm.registers().bp,
        step_counter: vm.step_counter(),
        call_depth: vm.call_depth(),
        output: vm.output().to_vec(),
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

fn write_length_prefixed_bytes(vm: &mut Tbx16Vm, addr: u16, bytes: &[u8]) {
    let addr = Address::new(addr);
    vm.memory_mut()
        .write_cell(addr, Cell::new(bytes.len() as u16))
        .unwrap();
    for (offset, byte) in bytes.iter().copied().enumerate() {
        vm.memory_mut()
            .write_byte(
                addr.checked_add(2)
                    .and_then(|start| start.checked_add_usize(offset))
                    .unwrap(),
                byte,
            )
            .unwrap();
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
fn primitive_registry_matches_the_m2_3b_abi_contract() {
    let mut ids = HashMap::new();
    let mut names = HashMap::new();
    for descriptor in PRIMITIVE_REGISTRY {
        assert!(ids.insert(descriptor.id as u16, descriptor.name).is_none());
        assert!(names
            .insert(descriptor.name, descriptor.id as u16)
            .is_none());
        assert_eq!(PrimitiveId::from_name(descriptor.name), Some(descriptor.id));
        assert_eq!(descriptor.id.descriptor(), descriptor);
        assert_eq!(primitive_descriptor_by_id(descriptor.id), descriptor);
        assert_eq!(
            primitive_descriptor_by_name(descriptor.name),
            Some(descriptor)
        );
        assert_eq!(
            PrimitiveId::try_from(descriptor.id.as_cell()),
            Ok(descriptor.id)
        );
    }

    assert_eq!(PrimitiveId::Lit as u16, 0);
    assert_eq!(PrimitiveId::Branch as u16, 1);
    assert_eq!(PrimitiveId::ZBranch as u16, 2);
    assert_eq!(PrimitiveId::Halt as u16, 3);
    assert_eq!(PrimitiveId::Exit as u16, 4);
    assert_eq!(PrimitiveId::Dup as u16, 16);
    assert_eq!(PrimitiveId::Add as u16, 32);
    assert_eq!(PrimitiveId::Eq as u16, 48);
    assert_eq!(PrimitiveId::ToBool as u16, 64);
    assert_eq!(PrimitiveId::Fetch as u16, 80);
    assert_eq!(PrimitiveId::Store as u16, 81);
    assert_eq!(PrimitiveId::PutChr as u16, 96);
    assert_eq!(PrimitiveId::PutDec as u16, 97);
    assert_eq!(PrimitiveId::PutStr as u16, 98);

    assert_eq!(PrimitiveId::Exit.operand(), PrimitiveOperand::Cell);
    assert_eq!(PrimitiveId::Dup.operand(), PrimitiveOperand::None);
    assert_eq!(PrimitiveId::PutDec.operand(), PrimitiveOperand::None);
    assert_eq!(PrimitiveId::from_name("add"), None);
    assert_eq!(primitive_descriptor_by_name("add"), None);
    assert_eq!(PrimitiveId::try_from(Cell::new(0xffff)), Err(()));

    let reversed: Vec<_> = PRIMITIVE_REGISTRY.iter().rev().copied().collect();
    for descriptor in PRIMITIVE_REGISTRY {
        assert_eq!(
            reversed
                .iter()
                .find(|candidate| candidate.id == descriptor.id),
            Some(descriptor)
        );
        assert_eq!(
            reversed
                .iter()
                .find(|candidate| candidate.name == descriptor.name),
            Some(descriptor)
        );
    }
}

#[test]
fn primitive_and_colon_xts_share_the_same_namespace() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    let colon_xt = image.colon_word(2, 3);
    image.load_into(&mut vm);

    assert_eq!(
        vm.resolve_xt(halt_xt).unwrap(),
        ResolvedWord::Primitive(PrimitiveId::Halt)
    );
    assert_eq!(
        vm.resolve_xt(colon_xt).unwrap(),
        ResolvedWord::Colon {
            arity: 2,
            local_count: 3,
            parameter_ip: Address::new(CODE_START + 10),
        }
    );
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
fn entry_colon_initializes_frames_for_multiple_arities_and_locals() {
    let mut zero_vm = Tbx16Vm::default();
    let mut zero_image = ImageBuilder::new(CODE_START);
    let halt_xt = zero_image.primitive(PrimitiveId::Halt);
    let entry_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(halt_xt);
    zero_image.load_into(&mut zero_vm);
    assert_eq!(zero_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(zero_vm.registers().bp, DATA_STACK_START);
    assert_eq!(zero_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(zero_vm.call_depth(), 0);

    let mut one_vm = Tbx16Vm::default();
    let mut one_image = ImageBuilder::new(CODE_START);
    let halt_xt = one_image.primitive(PrimitiveId::Halt);
    let entry_xt = one_image.colon_word(1, 0);
    one_image.emit_xt(halt_xt);
    one_image.load_into(&mut one_vm);
    one_vm.push_data_cell(Cell::new(0x1111)).unwrap();
    assert_eq!(one_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(one_vm.registers().bp, DATA_STACK_START);
    assert_eq!(one_vm.registers().dsp, Address::new(0x0082));

    let mut multi_vm = Tbx16Vm::default();
    let mut multi_image = ImageBuilder::new(CODE_START);
    let halt_xt = multi_image.primitive(PrimitiveId::Halt);
    let entry_xt = multi_image.colon_word(2, 2);
    multi_image.emit_xt(halt_xt);
    multi_image.load_into(&mut multi_vm);
    multi_vm.push_data_cell(Cell::new(0x1111)).unwrap();
    multi_vm.push_data_cell(Cell::new(0x2222)).unwrap();
    assert_eq!(multi_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(multi_vm.registers().bp, Address::new(0x0080));
    assert_eq!(multi_vm.registers().dsp, Address::new(0x0088));
    assert_eq!(
        multi_vm.memory().read_cell(Address::new(0x0084)).unwrap(),
        Cell::new(0)
    );
    assert_eq!(
        multi_vm.memory().read_cell(Address::new(0x0086)).unwrap(),
        Cell::new(0)
    );
}

#[test]
fn entry_colon_failures_are_atomic() {
    let mut arity_vm = Tbx16Vm::default();
    let mut arity_image = ImageBuilder::new(CODE_START);
    let entry_xt = arity_image.colon_word(1, 0);
    arity_image.load_into(&mut arity_vm);
    let before = snapshot(&arity_vm);
    assert_eq!(
        arity_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    let after = snapshot(&arity_vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.rsp, before.rsp);
    assert_eq!(after.bp, before.bp);
    assert_eq!(after.call_depth, before.call_depth);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.step_counter, 1);

    let mut overflow_vm = Tbx16Vm::default();
    let mut overflow_image = ImageBuilder::new(CODE_START);
    let entry_xt = overflow_image.colon_word(0, 65);
    overflow_image.load_into(&mut overflow_vm);
    let before = snapshot(&overflow_vm);
    assert_eq!(
        overflow_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackOverflow)
    );
    let after = snapshot(&overflow_vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.rsp, before.rsp);
    assert_eq!(after.bp, before.bp);
    assert_eq!(after.call_depth, before.call_depth);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.step_counter, 1);
}

#[test]
fn nested_colon_calls_build_frames_and_return_stack_layouts() {
    let mut zero_vm = Tbx16Vm::default();
    let mut zero_image = ImageBuilder::new(CODE_START);
    let halt_xt = zero_image.primitive(PrimitiveId::Halt);
    let callee_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(halt_xt);
    let entry_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(callee_xt);
    zero_image.load_into(&mut zero_vm);
    assert_eq!(zero_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(zero_vm.call_depth(), 1);
    assert_eq!(zero_vm.registers().rsp, Address::new(0x0204));
    assert_eq!(
        zero_vm.memory().read_cell(Address::new(0x0200)).unwrap(),
        Cell::new(CODE_START + 20)
    );
    assert_eq!(
        zero_vm.memory().read_cell(Address::new(0x0202)).unwrap(),
        Cell::new(DATA_STACK_START.get())
    );

    let mut one_vm = Tbx16Vm::default();
    let mut one_image = ImageBuilder::new(CODE_START);
    let halt_xt = one_image.primitive(PrimitiveId::Halt);
    let callee_xt = one_image.colon_word(1, 2);
    one_image.emit_xt(halt_xt);
    let entry_xt = one_image.colon_word(1, 0);
    one_image.emit_xt(callee_xt);
    one_image.load_into(&mut one_vm);
    one_vm.push_data_cell(Cell::new(0x4444)).unwrap();
    assert_eq!(one_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(one_vm.call_depth(), 1);
    assert_eq!(one_vm.registers().bp, DATA_STACK_START);
    assert_eq!(one_vm.registers().dsp, Address::new(0x0086));
    assert_eq!(
        one_vm.memory().read_cell(Address::new(0x0082)).unwrap(),
        Cell::new(0)
    );
    assert_eq!(
        one_vm.memory().read_cell(Address::new(0x0084)).unwrap(),
        Cell::new(0)
    );

    let mut multi_vm = Tbx16Vm::default();
    let mut multi_image = ImageBuilder::new(CODE_START);
    let halt_xt = multi_image.primitive(PrimitiveId::Halt);
    let callee_xt = multi_image.colon_word(2, 1);
    multi_image.emit_xt(halt_xt);
    let entry_xt = multi_image.colon_word(2, 0);
    multi_image.emit_xt(callee_xt);
    multi_image.load_into(&mut multi_vm);
    multi_vm.push_data_cell(Cell::new(0x1111)).unwrap();
    multi_vm.push_data_cell(Cell::new(0x2222)).unwrap();
    assert_eq!(multi_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(multi_vm.call_depth(), 1);
    assert_eq!(multi_vm.registers().bp, DATA_STACK_START);
    assert_eq!(multi_vm.registers().dsp, Address::new(0x0086));
    assert_eq!(
        multi_vm.memory().read_cell(Address::new(0x0084)).unwrap(),
        Cell::new(0)
    );
}

#[test]
fn nested_calls_account_for_return_stack_capacity_and_depth() {
    let region = StackRegion::new(Address::new(0x0200), Address::new(0x0204)).unwrap();
    let mut exact_vm = Tbx16Vm::new(region).unwrap();
    let mut exact_image = ImageBuilder::new(CODE_START);
    let halt_xt = exact_image.primitive(PrimitiveId::Halt);
    let callee_xt = exact_image.colon_word(0, 0);
    exact_image.emit_xt(halt_xt);
    let entry_xt = exact_image.colon_word(0, 0);
    exact_image.emit_xt(callee_xt);
    exact_image.load_into(&mut exact_vm);
    assert_eq!(exact_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(exact_vm.call_depth(), 1);
    assert_eq!(exact_vm.registers().rsp, region.end());

    let mut overflow_vm = Tbx16Vm::new(region).unwrap();
    let mut overflow_image = ImageBuilder::new(CODE_START);
    let halt_xt = overflow_image.primitive(PrimitiveId::Halt);
    let leaf_xt = overflow_image.colon_word(0, 0);
    overflow_image.emit_xt(halt_xt);
    let mid_xt = overflow_image.colon_word(0, 0);
    overflow_image.emit_xt(leaf_xt);
    let entry_xt = overflow_image.colon_word(0, 0);
    overflow_image.emit_xt(mid_xt);
    overflow_image.load_into(&mut overflow_vm);
    let before = snapshot(&overflow_vm);
    assert_eq!(
        overflow_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::ReturnStackOverflow)
    );
    assert_eq!(
        overflow_vm.registers().ip,
        Some(Address::new(CODE_START + 18))
    );
    assert_eq!(overflow_vm.registers().bp, DATA_STACK_START);
    assert_eq!(overflow_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(overflow_vm.registers().rsp, region.end());
    assert_eq!(overflow_vm.call_depth(), 1);
    assert_eq!(
        overflow_vm
            .memory()
            .read_cell(Address::new(0x0200))
            .unwrap(),
        Cell::new(CODE_START + 28)
    );
    assert_eq!(
        overflow_vm
            .memory()
            .read_cell(Address::new(0x0202))
            .unwrap(),
        Cell::new(DATA_STACK_START.get())
    );
    assert_eq!(before.step_counter, 0);
}

#[test]
fn nested_call_failures_leave_current_frame_state_unchanged() {
    let mut arity_vm = Tbx16Vm::default();
    let mut arity_image = ImageBuilder::new(CODE_START);
    let callee_xt = arity_image.colon_word(1, 0);
    let entry_xt = arity_image.colon_word(0, 0);
    arity_image.emit_xt(callee_xt);
    arity_image.load_into(&mut arity_vm);
    assert_eq!(
        arity_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    assert_eq!(arity_vm.registers().ip, Some(Address::new(CODE_START + 12)));
    assert_eq!(arity_vm.registers().bp, DATA_STACK_START);
    assert_eq!(arity_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(arity_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(arity_vm.call_depth(), 0);

    let mut locals_vm = Tbx16Vm::default();
    let mut locals_image = ImageBuilder::new(CODE_START);
    let callee_xt = locals_image.colon_word(0, 65);
    let entry_xt = locals_image.colon_word(0, 0);
    locals_image.emit_xt(callee_xt);
    locals_image.load_into(&mut locals_vm);
    assert_eq!(
        locals_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackOverflow)
    );
    assert_eq!(
        locals_vm.registers().ip,
        Some(Address::new(CODE_START + 12))
    );
    assert_eq!(locals_vm.registers().bp, DATA_STACK_START);
    assert_eq!(locals_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(locals_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(locals_vm.call_depth(), 0);
}

#[test]
fn three_nested_calls_match_return_stack_usage_and_call_depth() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    let level3_xt = image.colon_word(0, 0);
    image.emit_xt(halt_xt);
    let level2_xt = image.colon_word(0, 0);
    image.emit_xt(level3_xt);
    let level1_xt = image.colon_word(0, 0);
    image.emit_xt(level2_xt);
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(level1_xt);
    image.load_into(&mut vm);

    assert_eq!(vm.run(entry_xt), ExecutionOutcome::Halted);
    assert_eq!(vm.call_depth(), 3);
    assert_eq!(vm.registers().rsp, Address::new(0x020c));
    assert_eq!(
        vm.registers().rsp.get() - DEFAULT_RETURN_STACK_START.get(),
        vm.call_depth() * 4
    );
}

#[test]
fn threaded_lit_branch_and_zbranch_execute_handwritten_code() {
    let mut forward_vm = Tbx16Vm::default();
    let mut forward = ImageBuilder::new(CODE_START);
    let lit_xt = forward.primitive(PrimitiveId::Lit);
    let branch_xt = forward.primitive(PrimitiveId::Branch);
    let halt_xt = forward.primitive(PrimitiveId::Halt);
    forward.mark_label("start");
    forward.emit_xt(branch_xt);
    forward.emit_label_ref("target");
    forward.emit_xt(lit_xt);
    forward.emit_cell(Cell::new(0xdead));
    forward.mark_label("target");
    forward.emit_xt(lit_xt);
    forward.emit_cell(Cell::new(0x1234));
    forward.emit_xt(halt_xt);
    forward.load_into(&mut forward_vm);
    assert_eq!(
        forward_vm.run_threaded(Address::new(CODE_START + 12)),
        ExecutionOutcome::Halted
    );
    assert_eq!(forward_vm.peek_data_cell(0).unwrap(), Cell::new(0x1234));

    let mut backward_vm = Tbx16Vm::default();
    let mut backward = ImageBuilder::new(CODE_START);
    let lit_xt = backward.primitive(PrimitiveId::Lit);
    let branch_xt = backward.primitive(PrimitiveId::Branch);
    let halt_xt = backward.primitive(PrimitiveId::Halt);
    backward.mark_label("target");
    backward.emit_xt(lit_xt);
    backward.emit_cell(Cell::new(0x5678));
    backward.emit_xt(halt_xt);
    backward.mark_label("start");
    backward.emit_xt(branch_xt);
    backward.emit_label_ref("target");
    backward.load_into(&mut backward_vm);
    assert_eq!(
        backward_vm.run_threaded(Address::new(CODE_START + 18)),
        ExecutionOutcome::Halted
    );
    assert_eq!(backward_vm.peek_data_cell(0).unwrap(), Cell::new(0x5678));

    let mut zero_vm = Tbx16Vm::default();
    let mut zero = ImageBuilder::new(CODE_START);
    let lit_xt = zero.primitive(PrimitiveId::Lit);
    let zbranch_xt = zero.primitive(PrimitiveId::ZBranch);
    let halt_xt = zero.primitive(PrimitiveId::Halt);
    zero.mark_label("start");
    zero.emit_xt(lit_xt);
    zero.emit_cell(Cell::new(0));
    zero.emit_xt(zbranch_xt);
    zero.emit_label_ref("target");
    zero.emit_xt(lit_xt);
    zero.emit_cell(Cell::new(0x9999));
    zero.mark_label("target");
    zero.emit_xt(lit_xt);
    zero.emit_cell(Cell::new(0x2222));
    zero.emit_xt(halt_xt);
    zero.load_into(&mut zero_vm);
    assert_eq!(
        zero_vm.run_threaded(Address::new(CODE_START + 12)),
        ExecutionOutcome::Halted
    );
    assert_eq!(zero_vm.peek_data_cell(0).unwrap(), Cell::new(0x2222));

    let mut nonzero_vm = Tbx16Vm::default();
    let mut nonzero = ImageBuilder::new(CODE_START);
    let lit_xt = nonzero.primitive(PrimitiveId::Lit);
    let zbranch_xt = nonzero.primitive(PrimitiveId::ZBranch);
    let halt_xt = nonzero.primitive(PrimitiveId::Halt);
    nonzero.mark_label("start");
    nonzero.emit_xt(lit_xt);
    nonzero.emit_cell(Cell::new(1));
    nonzero.emit_xt(zbranch_xt);
    nonzero.emit_label_ref("target");
    nonzero.emit_xt(lit_xt);
    nonzero.emit_cell(Cell::new(0x3333));
    nonzero.emit_xt(halt_xt);
    nonzero.mark_label("target");
    nonzero.emit_xt(lit_xt);
    nonzero.emit_cell(Cell::new(0x4444));
    nonzero.emit_xt(halt_xt);
    nonzero.load_into(&mut nonzero_vm);
    assert_eq!(
        nonzero_vm.run_threaded(Address::new(CODE_START + 12)),
        ExecutionOutcome::Halted
    );
    assert_eq!(nonzero_vm.peek_data_cell(0).unwrap(), Cell::new(0x3333));
}

#[test]
fn entry_primitives_execute_with_normal_primitive_semantics_once() {
    let mut lit_vm = Tbx16Vm::default();
    let mut lit_image = ImageBuilder::new(CODE_START);
    let lit_xt = lit_image.primitive(PrimitiveId::Lit);
    lit_image.emit_cell(Cell::new(0x4321));
    lit_image.load_into(&mut lit_vm);
    lit_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    assert_eq!(lit_vm.run(lit_xt), ExecutionOutcome::Returned);
    assert_eq!(lit_vm.peek_data_cell(0).unwrap(), Cell::new(0x4321));

    let mut branch_vm = Tbx16Vm::default();
    let mut branch_image = ImageBuilder::new(CODE_START);
    let branch_xt = branch_image.primitive(PrimitiveId::Branch);
    branch_image.emit_cell(Cell::new(0x0410));
    branch_image.load_into(&mut branch_vm);
    branch_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    assert_eq!(branch_vm.run(branch_xt), ExecutionOutcome::Returned);
    assert_eq!(branch_vm.registers().ip, Some(Address::new(0x0410)));

    let mut zbranch_vm = Tbx16Vm::default();
    let mut zbranch_image = ImageBuilder::new(CODE_START);
    let zbranch_xt = zbranch_image.primitive(PrimitiveId::ZBranch);
    zbranch_image.emit_cell(Cell::new(0x0412));
    zbranch_image.load_into(&mut zbranch_vm);
    zbranch_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    zbranch_vm.push_data_cell(Cell::new(0)).unwrap();
    assert_eq!(zbranch_vm.run(zbranch_xt), ExecutionOutcome::Returned);
    assert_eq!(zbranch_vm.registers().ip, Some(Address::new(0x0412)));

    let mut halt_vm = Tbx16Vm::default();
    let mut halt_image = ImageBuilder::new(CODE_START);
    let halt_xt = halt_image.primitive(PrimitiveId::Halt);
    halt_image.load_into(&mut halt_vm);
    assert_eq!(halt_vm.run(halt_xt), ExecutionOutcome::Halted);

    let mut dup_vm = Tbx16Vm::default();
    let mut dup_image = ImageBuilder::new(CODE_START);
    let dup_xt = dup_image.primitive(PrimitiveId::Dup);
    dup_image.load_into(&mut dup_vm);
    dup_vm.push_data_cell(Cell::new(0x1234)).unwrap();
    assert_eq!(dup_vm.run(dup_xt), ExecutionOutcome::Returned);
    assert_eq!(dup_vm.step_counter(), 1);
    assert_eq!(dup_vm.peek_data_cell(0).unwrap(), Cell::new(0x1234));
    assert_eq!(dup_vm.peek_data_cell(1).unwrap(), Cell::new(0x1234));

    let mut add_vm = Tbx16Vm::default();
    let mut add_image = ImageBuilder::new(CODE_START);
    let add_xt = add_image.primitive(PrimitiveId::Add);
    add_image.load_into(&mut add_vm);
    add_vm.push_data_cell(Cell::new(1)).unwrap();
    add_vm.push_data_cell(Cell::new(2)).unwrap();
    assert_eq!(add_vm.run(add_xt), ExecutionOutcome::Returned);
    assert_eq!(add_vm.peek_data_cell(0).unwrap(), Cell::new(3));

    let mut fetch_vm = Tbx16Vm::default();
    let mut fetch_image = ImageBuilder::new(CODE_START);
    let fetch_xt = fetch_image.primitive(PrimitiveId::Fetch);
    fetch_image.load_into(&mut fetch_vm);
    fetch_vm
        .memory_mut()
        .write_cell(Address::new(0x3000), Cell::new(0xabcd))
        .unwrap();
    fetch_vm.push_data_cell(Cell::new(0x3000)).unwrap();
    assert_eq!(fetch_vm.run(fetch_xt), ExecutionOutcome::Returned);
    assert_eq!(fetch_vm.peek_data_cell(0).unwrap(), Cell::new(0xabcd));
}

#[test]
fn entry_primitives_trap_when_required_context_is_missing() {
    let mut lit_vm = Tbx16Vm::default();
    let mut lit_image = ImageBuilder::new(CODE_START);
    let lit_xt = lit_image.primitive(PrimitiveId::Lit);
    lit_image.load_into(&mut lit_vm);
    assert_eq!(
        lit_vm.run(lit_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0xffff),
        })
    );

    let mut zbranch_vm = Tbx16Vm::default();
    let mut zbranch_image = ImageBuilder::new(CODE_START);
    let zbranch_xt = zbranch_image.primitive(PrimitiveId::ZBranch);
    zbranch_image.emit_cell(Cell::new(0x0410));
    zbranch_image.load_into(&mut zbranch_vm);
    zbranch_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    assert_eq!(
        zbranch_vm.run(zbranch_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );

    for primitive in [
        PrimitiveId::Drop,
        PrimitiveId::Swap,
        PrimitiveId::Over,
        PrimitiveId::Add,
        PrimitiveId::Negate,
        PrimitiveId::Fetch,
        PrimitiveId::Store,
    ] {
        let mut vm = Tbx16Vm::default();
        let mut image = ImageBuilder::new(CODE_START);
        let xt = image.primitive(primitive);
        image.load_into(&mut vm);
        let before = snapshot(&vm);
        assert_eq!(
            vm.run(xt),
            ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
        );
        let after = snapshot(&vm);
        assert_eq!(after.ip, before.ip);
        assert_eq!(after.dsp, before.dsp);
        assert_eq!(after.memory, before.memory);
        assert_eq!(after.step_counter, 1);
    }
}

#[test]
fn stack_primitives_cover_normal_underflow_overflow_and_atomicity() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let dup_xt = image.primitive(PrimitiveId::Dup);
    let drop_xt = image.primitive(PrimitiveId::Drop);
    let swap_xt = image.primitive(PrimitiveId::Swap);
    let over_xt = image.primitive(PrimitiveId::Over);
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    assert_eq!(vm.run(dup_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x1111));
    assert_eq!(vm.peek_data_cell(1).unwrap(), Cell::new(0x1111));

    vm.push_data_cell(Cell::new(0x2222)).unwrap();
    assert_eq!(vm.run(swap_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x1111));
    assert_eq!(vm.peek_data_cell(1).unwrap(), Cell::new(0x2222));

    assert_eq!(vm.run(over_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x2222));
    assert_eq!(vm.peek_data_cell(1).unwrap(), Cell::new(0x1111));
    assert_eq!(vm.peek_data_cell(2).unwrap(), Cell::new(0x2222));

    assert_eq!(vm.run(drop_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x1111));

    let mut full_dup_vm = Tbx16Vm::default();
    let mut full_dup_image = ImageBuilder::new(CODE_START);
    let dup_xt = full_dup_image.primitive(PrimitiveId::Dup);
    full_dup_image.load_into(&mut full_dup_vm);
    for i in 0..64u16 {
        full_dup_vm.push_data_cell(Cell::new(i)).unwrap();
    }
    let before = snapshot(&full_dup_vm);
    assert_eq!(
        full_dup_vm.run(dup_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackOverflow)
    );
    let after = snapshot(&full_dup_vm);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);

    let mut swap_vm = Tbx16Vm::default();
    let mut swap_image = ImageBuilder::new(CODE_START);
    let swap_xt = swap_image.primitive(PrimitiveId::Swap);
    swap_image.load_into(&mut swap_vm);
    swap_vm.push_data_cell(Cell::new(0xaaaa)).unwrap();
    let before = snapshot(&swap_vm);
    assert_eq!(
        swap_vm.run(swap_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    let after = snapshot(&swap_vm);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);
}

#[test]
fn arithmetic_primitives_cover_wrap_signed_edges_and_division_by_zero() {
    let arithmetic_cases = [
        (
            PrimitiveId::Add,
            Cell::new(0xffff),
            Cell::new(0x0001),
            Cell::new(0x0000),
        ),
        (
            PrimitiveId::Sub,
            Cell::new(0x0000),
            Cell::new(0x0001),
            Cell::new(0xffff),
        ),
        (
            PrimitiveId::Mul,
            Cell::from_i16(300),
            Cell::from_i16(300),
            Cell::from_i16(24464),
        ),
    ];
    for (primitive, lhs, rhs, expected) in arithmetic_cases {
        let mut vm = Tbx16Vm::default();
        let mut image = ImageBuilder::new(CODE_START);
        let xt = image.primitive(primitive);
        image.load_into(&mut vm);
        vm.push_data_cell(lhs).unwrap();
        vm.push_data_cell(rhs).unwrap();
        assert_eq!(vm.run(xt), ExecutionOutcome::Returned);
        assert_eq!(vm.peek_data_cell(0).unwrap(), expected);
    }

    let mut negate_vm = Tbx16Vm::default();
    let mut negate_image = ImageBuilder::new(CODE_START);
    let negate_xt = negate_image.primitive(PrimitiveId::Negate);
    negate_image.load_into(&mut negate_vm);
    negate_vm.push_data_cell(Cell::from_i16(i16::MIN)).unwrap();
    assert_eq!(negate_vm.run(negate_xt), ExecutionOutcome::Returned);
    assert_eq!(
        negate_vm.peek_data_cell(0).unwrap(),
        Cell::from_i16(i16::MIN)
    );

    let signed_cases = [
        (7, 3, 2, 1),
        (7, -3, -2, 1),
        (-7, 3, -2, -1),
        (-7, -3, 2, -1),
    ];
    for (lhs, rhs, quotient, remainder) in signed_cases {
        let mut div_vm = Tbx16Vm::default();
        let mut div_image = ImageBuilder::new(CODE_START);
        let div_xt = div_image.primitive(PrimitiveId::Div);
        div_image.load_into(&mut div_vm);
        div_vm.push_data_cell(Cell::from_i16(lhs)).unwrap();
        div_vm.push_data_cell(Cell::from_i16(rhs)).unwrap();
        assert_eq!(div_vm.run(div_xt), ExecutionOutcome::Returned);
        assert_eq!(div_vm.peek_data_cell(0).unwrap(), Cell::from_i16(quotient));

        let mut mod_vm = Tbx16Vm::default();
        let mut mod_image = ImageBuilder::new(CODE_START);
        let mod_xt = mod_image.primitive(PrimitiveId::Mod);
        mod_image.load_into(&mut mod_vm);
        mod_vm.push_data_cell(Cell::from_i16(lhs)).unwrap();
        mod_vm.push_data_cell(Cell::from_i16(rhs)).unwrap();
        assert_eq!(mod_vm.run(mod_xt), ExecutionOutcome::Returned);
        assert_eq!(mod_vm.peek_data_cell(0).unwrap(), Cell::from_i16(remainder));
    }

    let mut min_vm = Tbx16Vm::default();
    let mut min_image = ImageBuilder::new(CODE_START);
    let div_xt = min_image.primitive(PrimitiveId::Div);
    let mod_xt = min_image.primitive(PrimitiveId::Mod);
    min_image.load_into(&mut min_vm);
    min_vm.push_data_cell(Cell::from_i16(i16::MIN)).unwrap();
    min_vm.push_data_cell(Cell::from_i16(-1)).unwrap();
    assert_eq!(min_vm.run(div_xt), ExecutionOutcome::Returned);
    assert_eq!(min_vm.peek_data_cell(0).unwrap(), Cell::from_i16(i16::MIN));
    min_vm.push_data_cell(Cell::from_i16(i16::MIN)).unwrap();
    min_vm.push_data_cell(Cell::from_i16(-1)).unwrap();
    assert_eq!(min_vm.run(mod_xt), ExecutionOutcome::Returned);
    assert_eq!(min_vm.peek_data_cell(0).unwrap(), Cell::new(0));

    for primitive in [PrimitiveId::Div, PrimitiveId::Mod] {
        let mut vm = Tbx16Vm::default();
        let mut image = ImageBuilder::new(CODE_START);
        let xt = image.primitive(primitive);
        image.load_into(&mut vm);
        vm.push_data_cell(Cell::new(0x9999)).unwrap();
        vm.push_data_cell(Cell::new(0)).unwrap();
        let before = snapshot(&vm);
        assert_eq!(
            vm.run(xt),
            ExecutionOutcome::Trapped(Tbx16Error::DivisionByZero)
        );
        let after = snapshot(&vm);
        assert_eq!(after.dsp, before.dsp);
        assert_eq!(after.memory, before.memory);
        assert_eq!(after.step_counter, 1);
    }
}

#[test]
fn comparison_and_logical_primitives_return_canonical_booleans() {
    let comparison_cases = [
        (
            PrimitiveId::Eq,
            Cell::new(0x1234),
            Cell::new(0x1234),
            Cell::TRUE,
        ),
        (
            PrimitiveId::Ne,
            Cell::new(0x1234),
            Cell::new(0x1235),
            Cell::TRUE,
        ),
        (
            PrimitiveId::Lt,
            Cell::from_i16(-1),
            Cell::from_i16(1),
            Cell::TRUE,
        ),
        (
            PrimitiveId::Le,
            Cell::from_i16(4),
            Cell::from_i16(4),
            Cell::TRUE,
        ),
        (
            PrimitiveId::Gt,
            Cell::from_i16(5),
            Cell::from_i16(4),
            Cell::TRUE,
        ),
        (
            PrimitiveId::Ge,
            Cell::from_i16(-4),
            Cell::from_i16(-4),
            Cell::TRUE,
        ),
    ];
    for (primitive, lhs, rhs, expected) in comparison_cases {
        let mut vm = Tbx16Vm::default();
        let mut image = ImageBuilder::new(CODE_START);
        let xt = image.primitive(primitive);
        image.load_into(&mut vm);
        vm.push_data_cell(lhs).unwrap();
        vm.push_data_cell(rhs).unwrap();
        assert_eq!(vm.run(xt), ExecutionOutcome::Returned);
        assert_eq!(vm.peek_data_cell(0).unwrap(), expected);
    }

    let truthy_values = [
        Cell::new(0),
        Cell::new(1),
        Cell::new(0x8000),
        Cell::new(0xffff),
    ];
    for value in truthy_values {
        let mut vm = Tbx16Vm::default();
        let mut image = ImageBuilder::new(CODE_START);
        let xt = image.primitive(PrimitiveId::ToBool);
        image.load_into(&mut vm);
        vm.push_data_cell(value).unwrap();
        assert_eq!(vm.run(xt), ExecutionOutcome::Returned);
        assert_eq!(vm.peek_data_cell(0).unwrap(), value.to_canonical_bool());
    }

    let mut not_vm = Tbx16Vm::default();
    let mut not_image = ImageBuilder::new(CODE_START);
    let not_xt = not_image.primitive(PrimitiveId::Not);
    not_image.load_into(&mut not_vm);
    not_vm.push_data_cell(Cell::new(0)).unwrap();
    assert_eq!(not_vm.run(not_xt), ExecutionOutcome::Returned);
    assert_eq!(not_vm.peek_data_cell(0).unwrap(), Cell::TRUE);
    not_vm.push_data_cell(Cell::new(0x8000)).unwrap();
    assert_eq!(not_vm.run(not_xt), ExecutionOutcome::Returned);
    assert_eq!(not_vm.peek_data_cell(0).unwrap(), Cell::FALSE);

    let mut and_vm = Tbx16Vm::default();
    let mut and_image = ImageBuilder::new(CODE_START);
    let and_xt = and_image.primitive(PrimitiveId::And);
    and_image.load_into(&mut and_vm);
    and_vm.push_data_cell(Cell::new(0x8000)).unwrap();
    and_vm.push_data_cell(Cell::new(0x0001)).unwrap();
    assert_eq!(and_vm.run(and_xt), ExecutionOutcome::Returned);
    assert_eq!(and_vm.peek_data_cell(0).unwrap(), Cell::TRUE);

    let mut or_vm = Tbx16Vm::default();
    let mut or_image = ImageBuilder::new(CODE_START);
    let or_xt = or_image.primitive(PrimitiveId::Or);
    or_image.load_into(&mut or_vm);
    or_vm.push_data_cell(Cell::new(0)).unwrap();
    or_vm.push_data_cell(Cell::new(0xffff)).unwrap();
    assert_eq!(or_vm.run(or_xt), ExecutionOutcome::Returned);
    assert_eq!(or_vm.peek_data_cell(0).unwrap(), Cell::TRUE);

    let mut band_vm = Tbx16Vm::default();
    let mut band_image = ImageBuilder::new(CODE_START);
    let band_xt = band_image.primitive(PrimitiveId::Band);
    band_image.load_into(&mut band_vm);
    band_vm.push_data_cell(Cell::new(0x8000)).unwrap();
    band_vm.push_data_cell(Cell::new(0x0001)).unwrap();
    assert_eq!(band_vm.run(band_xt), ExecutionOutcome::Returned);
    assert_eq!(band_vm.peek_data_cell(0).unwrap(), Cell::FALSE);

    let mut bor_vm = Tbx16Vm::default();
    let mut bor_image = ImageBuilder::new(CODE_START);
    let bor_xt = bor_image.primitive(PrimitiveId::Bor);
    bor_image.load_into(&mut bor_vm);
    bor_vm.push_data_cell(Cell::new(0x8000)).unwrap();
    bor_vm.push_data_cell(Cell::new(0x0001)).unwrap();
    assert_eq!(bor_vm.run(bor_xt), ExecutionOutcome::Returned);
    assert_eq!(bor_vm.peek_data_cell(0).unwrap(), Cell::new(0x8001));
}

#[test]
fn memory_primitives_cover_little_endian_odd_addresses_and_atomic_traps() {
    let mut store_vm = Tbx16Vm::default();
    let mut store_image = ImageBuilder::new(CODE_START);
    let store_xt = store_image.primitive(PrimitiveId::Store);
    store_image.load_into(&mut store_vm);
    store_vm.push_data_cell(Cell::new(0x3001)).unwrap();
    store_vm.push_data_cell(Cell::new(0xabcd)).unwrap();
    assert_eq!(store_vm.run(store_xt), ExecutionOutcome::Returned);
    assert_eq!(
        store_vm.memory().read_byte(Address::new(0x3001)).unwrap(),
        0xcd
    );
    assert_eq!(
        store_vm.memory().read_byte(Address::new(0x3002)).unwrap(),
        0xab
    );

    let mut fetch_vm = Tbx16Vm::default();
    let mut fetch_image = ImageBuilder::new(CODE_START);
    let fetch_xt = fetch_image.primitive(PrimitiveId::Fetch);
    fetch_image.load_into(&mut fetch_vm);
    fetch_vm
        .memory_mut()
        .write_cell(Address::new(0x3001), Cell::new(0xabcd))
        .unwrap();
    fetch_vm.push_data_cell(Cell::new(0x3001)).unwrap();
    assert_eq!(fetch_vm.run(fetch_xt), ExecutionOutcome::Returned);
    assert_eq!(fetch_vm.peek_data_cell(0).unwrap(), Cell::new(0xabcd));

    let mut trap_fetch_vm = Tbx16Vm::default();
    let mut trap_fetch_image = ImageBuilder::new(CODE_START);
    let fetch_xt = trap_fetch_image.primitive(PrimitiveId::Fetch);
    trap_fetch_image.load_into(&mut trap_fetch_vm);
    trap_fetch_vm.push_data_cell(Cell::new(0xffff)).unwrap();
    let before = snapshot(&trap_fetch_vm);
    assert_eq!(
        trap_fetch_vm.run(fetch_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidMemoryAccess {
            addr: Address::new(0xffff),
            operation: "cell read",
        })
    );
    let after = snapshot(&trap_fetch_vm);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);

    let mut trap_store_vm = Tbx16Vm::default();
    let mut trap_store_image = ImageBuilder::new(CODE_START);
    let store_xt = trap_store_image.primitive(PrimitiveId::Store);
    trap_store_image.load_into(&mut trap_store_vm);
    trap_store_vm.push_data_cell(Cell::new(0xffff)).unwrap();
    trap_store_vm.push_data_cell(Cell::new(0x1234)).unwrap();
    let before = snapshot(&trap_store_vm);
    assert_eq!(
        trap_store_vm.run(store_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidMemoryAccess {
            addr: Address::new(0xffff),
            operation: "cell write",
        })
    );
    let after = snapshot(&trap_store_vm);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);
}

#[test]
fn output_sink_supports_borrow_take_and_clear() {
    let mut vm = Tbx16Vm::default();
    assert_eq!(vm.output(), b"");

    write_length_prefixed_bytes(&mut vm, 0x3000, b"abc");
    let mut image = ImageBuilder::new(CODE_START);
    let putstr_xt = image.primitive(PrimitiveId::PutStr);
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(0x3000)).unwrap();
    assert_eq!(vm.run(putstr_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.output(), b"abc");

    let taken = vm.take_output();
    assert_eq!(taken, b"abc");
    assert_eq!(vm.output(), b"");

    vm.push_data_cell(Cell::new(0x3000)).unwrap();
    assert_eq!(vm.run(putstr_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.output(), b"abc");
    vm.clear_output();
    assert_eq!(vm.output(), b"");
}

#[test]
fn output_sink_accumulates_across_runs_and_survives_reset() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let putchr_xt = image.primitive(PrimitiveId::PutChr);
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(u16::from(b'A'))).unwrap();
    assert_eq!(vm.run(putchr_xt), ExecutionOutcome::Returned);
    vm.push_data_cell(Cell::new(u16::from(b'B'))).unwrap();
    assert_eq!(vm.run(putchr_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.output(), b"AB");

    vm.reset_execution_state();
    assert_eq!(vm.output(), b"AB");

    vm.push_data_cell(Cell::new(u16::from(b'C'))).unwrap();
    assert_eq!(vm.run(putchr_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.take_output(), b"ABC");
}

#[test]
fn output_primitives_cover_entry_threaded_and_atomic_traps() {
    let mut chr_vm = Tbx16Vm::default();
    let mut chr_image = ImageBuilder::new(CODE_START);
    let putchr_xt = chr_image.primitive(PrimitiveId::PutChr);
    chr_image.load_into(&mut chr_vm);
    for value in [0x0041, 0xff41, 0x0000] {
        chr_vm.push_data_cell(Cell::new(value)).unwrap();
        assert_eq!(chr_vm.run(putchr_xt), ExecutionOutcome::Returned);
    }
    assert_eq!(chr_vm.take_output(), vec![b'A', b'A', 0x00]);

    let mut dec_vm = Tbx16Vm::default();
    let mut dec_image = ImageBuilder::new(CODE_START);
    let putdec_xt = dec_image.primitive(PrimitiveId::PutDec);
    dec_image.load_into(&mut dec_vm);
    for value in [0, 1234, -45, -32768] {
        dec_vm.push_data_cell(Cell::from_i16(value)).unwrap();
        assert_eq!(dec_vm.run(putdec_xt), ExecutionOutcome::Returned);
    }
    assert_eq!(dec_vm.take_output(), b"01234-45-32768");

    let mut str_vm = Tbx16Vm::default();
    let mut str_image = ImageBuilder::new(CODE_START);
    let putstr_xt = str_image.primitive(PrimitiveId::PutStr);
    str_image.load_into(&mut str_vm);
    write_length_prefixed_bytes(&mut str_vm, 0x3001, b"hi\0");
    str_vm.push_data_cell(Cell::new(0x3001)).unwrap();
    assert_eq!(str_vm.run(putstr_xt), ExecutionOutcome::Returned);
    assert_eq!(str_vm.take_output(), b"hi\0");

    let mut threaded_vm = Tbx16Vm::default();
    let mut threaded_image = ImageBuilder::new(CODE_START);
    let lit_xt = threaded_image.primitive(PrimitiveId::Lit);
    let putchr_xt = threaded_image.primitive(PrimitiveId::PutChr);
    let putdec_xt = threaded_image.primitive(PrimitiveId::PutDec);
    let putstr_xt = threaded_image.primitive(PrimitiveId::PutStr);
    let halt_xt = threaded_image.primitive(PrimitiveId::Halt);
    write_length_prefixed_bytes(&mut threaded_vm, 0x3100, b"!");
    threaded_image.emit_xt(lit_xt);
    threaded_image.emit_cell(Cell::new(u16::from(b'A')));
    threaded_image.emit_xt(putchr_xt);
    threaded_image.emit_xt(lit_xt);
    threaded_image.emit_cell(Cell::from_i16(-12));
    threaded_image.emit_xt(putdec_xt);
    threaded_image.emit_xt(lit_xt);
    threaded_image.emit_cell(Cell::new(0x3100));
    threaded_image.emit_xt(putstr_xt);
    threaded_image.emit_xt(halt_xt);
    threaded_image.load_into(&mut threaded_vm);
    assert_eq!(
        threaded_vm.run_threaded(Address::new(CODE_START + 20)),
        ExecutionOutcome::Halted
    );
    assert_eq!(threaded_vm.take_output(), b"A-12!");

    for primitive in [
        PrimitiveId::PutChr,
        PrimitiveId::PutDec,
        PrimitiveId::PutStr,
    ] {
        let mut vm = Tbx16Vm::default();
        let mut image = ImageBuilder::new(CODE_START);
        let xt = image.primitive(primitive);
        image.load_into(&mut vm);
        let before = snapshot(&vm);
        assert_eq!(
            vm.run(xt),
            ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
        );
        let after = snapshot(&vm);
        assert_eq!(after.dsp, before.dsp);
        assert_eq!(after.output, before.output);
        assert_eq!(after.memory, before.memory);
        assert_eq!(after.step_counter, 1);
    }
}

#[test]
fn putstr_rejects_end_crossing_without_mutation() {
    let mut zero_len_vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let putstr_xt = image.primitive(PrimitiveId::PutStr);
    image.load_into(&mut zero_len_vm);
    zero_len_vm
        .memory_mut()
        .write_cell(Address::new(0xfffe), Cell::new(0))
        .unwrap();
    zero_len_vm.push_data_cell(Cell::new(0xfffe)).unwrap();
    assert_eq!(zero_len_vm.run(putstr_xt), ExecutionOutcome::Returned);
    assert_eq!(zero_len_vm.output(), b"");
    assert_eq!(zero_len_vm.registers().dsp, DATA_STACK_START);

    let mut length_vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let putstr_xt = image.primitive(PrimitiveId::PutStr);
    image.load_into(&mut length_vm);
    length_vm.push_data_cell(Cell::new(0xffff)).unwrap();
    let before = snapshot(&length_vm);
    assert_eq!(
        length_vm.run(putstr_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidMemoryAccess {
            addr: Address::new(0xffff),
            operation: "cell read",
        })
    );
    let after = snapshot(&length_vm);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.output, before.output);
    assert_eq!(after.memory, before.memory);

    let mut payload_vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let putstr_xt = image.primitive(PrimitiveId::PutStr);
    image.load_into(&mut payload_vm);
    payload_vm
        .memory_mut()
        .write_cell(Address::new(0xfffd), Cell::new(2))
        .unwrap();
    payload_vm
        .memory_mut()
        .write_byte(Address::new(0xffff), b'X')
        .unwrap();
    payload_vm.push_data_cell(Cell::new(0xfffd)).unwrap();
    let before = snapshot(&payload_vm);
    assert_eq!(
        payload_vm.run(putstr_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidMemoryAccess {
            addr: Address::new(0xfffd),
            operation: "string read",
        })
    );
    let after = snapshot(&payload_vm);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.output, before.output);
    assert_eq!(after.memory, before.memory);
}

#[test]
fn threaded_program_executes_m2_3a_primitives() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let add_xt = image.primitive(PrimitiveId::Add);
    let dup_xt = image.primitive(PrimitiveId::Dup);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    image.mark_label("start");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(2));
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(3));
    image.emit_xt(add_xt);
    image.emit_xt(dup_xt);
    image.emit_xt(halt_xt);
    image.load_into(&mut vm);

    assert_eq!(
        vm.run_threaded(Address::new(CODE_START + 16)),
        ExecutionOutcome::Halted
    );
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(5));
    assert_eq!(vm.peek_data_cell(1).unwrap(), Cell::new(5));
}

#[test]
fn invalid_branch_targets_trap_for_threaded_and_entry_execution() {
    let mut odd_vm = Tbx16Vm::default();
    let mut odd_image = ImageBuilder::new(CODE_START);
    let branch_xt = odd_image.primitive(PrimitiveId::Branch);
    odd_image.mark_label("start");
    odd_image.emit_xt(branch_xt);
    odd_image.emit_cell(Cell::new(0x0401));
    odd_image.load_into(&mut odd_vm);
    assert_eq!(
        odd_vm.run_threaded(Address::new(CODE_START + 4)),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0x0401),
        })
    );

    let mut ffff_vm = Tbx16Vm::default();
    let mut ffff_image = ImageBuilder::new(CODE_START);
    let branch_xt = ffff_image.primitive(PrimitiveId::Branch);
    ffff_image.emit_cell(Cell::new(0xffff));
    ffff_image.load_into(&mut ffff_vm);
    ffff_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    assert_eq!(
        ffff_vm.run(branch_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0xffff),
        })
    );
}

#[test]
fn step_limit_stops_before_the_next_threaded_dispatch() {
    let mut limited_vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    image.mark_label("start");
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x1111));
    image.emit_xt(halt_xt);
    image.load_into(&mut limited_vm);

    limited_vm.set_step_limit(Some(1));
    assert_eq!(
        limited_vm.run_threaded(Address::new(CODE_START + 8)),
        ExecutionOutcome::Trapped(Tbx16Error::StepLimitExceeded)
    );
    assert_eq!(limited_vm.step_counter(), 1);
    assert_eq!(
        limited_vm.registers().ip,
        Some(Address::new(CODE_START + 12))
    );

    let mut ok_vm = Tbx16Vm::default();
    let mut ok_image = ImageBuilder::new(CODE_START);
    let lit_xt = ok_image.primitive(PrimitiveId::Lit);
    let halt_xt = ok_image.primitive(PrimitiveId::Halt);
    ok_image.emit_xt(lit_xt);
    ok_image.emit_cell(Cell::new(0x1111));
    ok_image.emit_xt(halt_xt);
    ok_image.load_into(&mut ok_vm);
    ok_vm.set_step_limit(Some(2));
    assert_eq!(
        ok_vm.run_threaded(Address::new(CODE_START + 8)),
        ExecutionOutcome::Halted
    );
    assert_eq!(ok_vm.step_counter(), 2);
}

#[test]
fn threaded_failures_after_fetch_still_increment_step_counter() {
    let mut invalid_xt_vm = Tbx16Vm::default();
    invalid_xt_vm
        .memory_mut()
        .write_cell(Address::new(CODE_START), Cell::new(0x9999))
        .unwrap();
    assert_eq!(
        invalid_xt_vm.run_threaded(Address::new(CODE_START)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(0x9999),
        })
    );
    assert_eq!(invalid_xt_vm.step_counter(), 1);

    let mut lit_vm = Tbx16Vm::default();
    let mut lit_image = ImageBuilder::new(CODE_START);
    let lit_xt = lit_image.primitive(PrimitiveId::Lit);
    lit_image.emit_xt(lit_xt);
    lit_image.emit_cell(Cell::new(0x1111));
    lit_image.load_into(&mut lit_vm);
    for i in 0..64u16 {
        lit_vm.push_data_cell(Cell::new(i)).unwrap();
    }
    assert_eq!(
        lit_vm.run_threaded(Address::new(CODE_START + 4)),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackOverflow)
    );
    assert_eq!(lit_vm.step_counter(), 1);

    let mut zbranch_vm = Tbx16Vm::default();
    let mut zbranch_image = ImageBuilder::new(CODE_START);
    let zbranch_xt = zbranch_image.primitive(PrimitiveId::ZBranch);
    zbranch_image.emit_xt(zbranch_xt);
    zbranch_image.emit_cell(Cell::new(0x0410));
    zbranch_image.load_into(&mut zbranch_vm);
    assert_eq!(
        zbranch_vm.run_threaded(Address::new(CODE_START + 4)),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    assert_eq!(zbranch_vm.step_counter(), 1);
}

#[test]
fn threaded_atomic_failures_preserve_ip_stack_and_memory() {
    let mut invalid_xt_vm = Tbx16Vm::default();
    invalid_xt_vm
        .memory_mut()
        .write_cell(Address::new(CODE_START), Cell::new(0x9999))
        .unwrap();
    invalid_xt_vm
        .set_instruction_pointer(Address::new(CODE_START))
        .unwrap();
    let before = snapshot(&invalid_xt_vm);
    assert_eq!(
        invalid_xt_vm.run_threaded(Address::new(CODE_START)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionToken {
            xt: Cell::new(0x9999),
        })
    );
    let after = snapshot(&invalid_xt_vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.step_counter, 1);

    let mut lit_vm = Tbx16Vm::default();
    let mut lit_image = ImageBuilder::new(CODE_START);
    let lit_xt = lit_image.primitive(PrimitiveId::Lit);
    lit_image.emit_xt(lit_xt);
    lit_image.emit_cell(Cell::new(0x5555));
    lit_image.load_into(&mut lit_vm);
    for i in 0..64u16 {
        lit_vm.push_data_cell(Cell::new(i)).unwrap();
    }
    lit_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    let before = snapshot(&lit_vm);
    assert_eq!(
        lit_vm.run_threaded(Address::new(CODE_START + 4)),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackOverflow)
    );
    let after = snapshot(&lit_vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.step_counter, 1);

    let mut branch_vm = Tbx16Vm::default();
    let mut branch_image = ImageBuilder::new(CODE_START);
    let branch_xt = branch_image.primitive(PrimitiveId::Branch);
    branch_image.emit_xt(branch_xt);
    branch_image.emit_cell(Cell::new(0xffff));
    branch_image.load_into(&mut branch_vm);
    branch_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    let before = snapshot(&branch_vm);
    assert_eq!(
        branch_vm.run_threaded(Address::new(CODE_START + 4)),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0xffff),
        })
    );
    let after = snapshot(&branch_vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.step_counter, 1);

    let mut zbranch_vm = Tbx16Vm::default();
    let mut zbranch_image = ImageBuilder::new(CODE_START);
    let zbranch_xt = zbranch_image.primitive(PrimitiveId::ZBranch);
    zbranch_image.emit_xt(zbranch_xt);
    zbranch_image.emit_cell(Cell::new(0x0410));
    zbranch_image.load_into(&mut zbranch_vm);
    zbranch_vm
        .set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    let before = snapshot(&zbranch_vm);
    assert_eq!(
        zbranch_vm.run_threaded(Address::new(CODE_START + 4)),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    let after = snapshot(&zbranch_vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.step_counter, 1);
}

#[test]
fn top_level_exit_zero_and_one_return_cleanly() {
    let mut zero_vm = Tbx16Vm::default();
    let mut zero_image = ImageBuilder::new(CODE_START);
    let exit_xt = zero_image.primitive(PrimitiveId::Exit);
    let entry_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(exit_xt);
    zero_image.emit_cell(Cell::new(0));
    zero_image.load_into(&mut zero_vm);

    assert_eq!(zero_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(zero_vm.registers().ip, None);
    assert_eq!(zero_vm.registers().bp, DATA_STACK_START);
    assert_eq!(zero_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(zero_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(zero_vm.call_depth(), 0);
    assert!(!zero_vm.is_dirty_execution_state());

    let mut one_vm = Tbx16Vm::default();
    let mut one_image = ImageBuilder::new(CODE_START);
    let lit_xt = one_image.primitive(PrimitiveId::Lit);
    let exit_xt = one_image.primitive(PrimitiveId::Exit);
    let entry_xt = one_image.colon_word(1, 0);
    one_image.emit_xt(lit_xt);
    one_image.emit_cell(Cell::new(0x4444));
    one_image.emit_xt(exit_xt);
    one_image.emit_cell(Cell::new(1));
    one_image.load_into(&mut one_vm);
    one_vm.push_data_cell(Cell::new(0x1111)).unwrap();
    one_vm.push_data_cell(Cell::new(0x2222)).unwrap();

    assert_eq!(one_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(one_vm.registers().ip, None);
    assert_eq!(one_vm.registers().bp, DATA_STACK_START);
    assert_eq!(one_vm.registers().dsp, Address::new(0x0084));
    assert_eq!(one_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(one_vm.call_depth(), 0);
    assert_eq!(
        one_vm.memory().read_cell(Address::new(0x0080)).unwrap(),
        Cell::new(0x1111)
    );
    assert_eq!(
        one_vm.memory().read_cell(Address::new(0x0082)).unwrap(),
        Cell::new(0x4444)
    );
    assert!(!one_vm.is_dirty_execution_state());
}

#[test]
fn nested_exit_zero_and_one_restore_caller_frame() {
    let mut zero_vm = Tbx16Vm::default();
    let mut zero_image = ImageBuilder::new(CODE_START);
    let lit_xt = zero_image.primitive(PrimitiveId::Lit);
    let exit_xt = zero_image.primitive(PrimitiveId::Exit);
    let callee_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(exit_xt);
    zero_image.emit_cell(Cell::new(0));
    let entry_xt = zero_image.colon_word(0, 0);
    zero_image.emit_xt(lit_xt);
    zero_image.emit_cell(Cell::new(0x7777));
    zero_image.emit_xt(callee_xt);
    zero_image.emit_xt(exit_xt);
    zero_image.emit_cell(Cell::new(0));
    zero_image.load_into(&mut zero_vm);

    assert_eq!(zero_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(zero_vm.registers().dsp, DATA_STACK_START);
    assert_eq!(zero_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(zero_vm.call_depth(), 0);

    let mut one_vm = Tbx16Vm::default();
    let mut one_image = ImageBuilder::new(CODE_START);
    let lit_xt = one_image.primitive(PrimitiveId::Lit);
    let exit_xt = one_image.primitive(PrimitiveId::Exit);
    let callee_xt = one_image.colon_word(1, 0);
    one_image.emit_xt(lit_xt);
    one_image.emit_cell(Cell::new(0x5555));
    one_image.emit_xt(exit_xt);
    one_image.emit_cell(Cell::new(1));
    let entry_xt = one_image.colon_word(0, 0);
    one_image.emit_xt(lit_xt);
    one_image.emit_cell(Cell::new(0x1234));
    one_image.emit_xt(callee_xt);
    one_image.emit_xt(exit_xt);
    one_image.emit_cell(Cell::new(1));
    one_image.load_into(&mut one_vm);

    assert_eq!(one_vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(one_vm.registers().ip, None);
    assert_eq!(one_vm.registers().bp, DATA_STACK_START);
    assert_eq!(one_vm.registers().dsp, Address::new(0x0082));
    assert_eq!(one_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(one_vm.call_depth(), 0);
    assert_eq!(one_vm.peek_data_cell(0).unwrap(), Cell::new(0x5555));
    assert!(!one_vm.is_dirty_execution_state());
}

#[test]
fn three_nested_exit_one_returns_restore_full_state() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let exit_xt = image.primitive(PrimitiveId::Exit);

    let level3_xt = image.colon_word(0, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x3333));
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let level2_xt = image.colon_word(0, 0);
    image.emit_xt(level3_xt);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let level1_xt = image.colon_word(0, 0);
    image.emit_xt(level2_xt);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));

    let entry_xt = image.colon_word(1, 0);
    image.emit_xt(level1_xt);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    vm.push_data_cell(Cell::new(0x2222)).unwrap();

    assert_eq!(vm.run(entry_xt), ExecutionOutcome::Returned);
    assert_eq!(vm.registers().ip, None);
    assert_eq!(vm.registers().bp, DATA_STACK_START);
    assert_eq!(vm.registers().dsp, Address::new(0x0084));
    assert_eq!(vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(vm.call_depth(), 0);
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0080)).unwrap(),
        Cell::new(0x1111)
    );
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x3333));
    assert!(!vm.is_dirty_execution_state());
}

#[test]
fn exit_one_rejects_missing_callee_return_value_without_stealing_caller_value() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let exit_xt = image.primitive(PrimitiveId::Exit);
    let callee_xt = image.colon_word(0, 0);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(1));
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(callee_xt);
    image.emit_xt(exit_xt);
    image.emit_cell(Cell::new(0));
    image.load_into(&mut vm);
    vm.push_data_cell(Cell::new(0xaaaa)).unwrap();
    let before = snapshot(&vm);

    assert_eq!(
        vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DataStackUnderflow)
    );
    let after = snapshot(&vm);
    assert_eq!(after.ip, Some(Address::new(CODE_START + 10)));
    assert_eq!(after.dsp, Address::new(0x0082));
    assert_eq!(after.rsp, Address::new(0x0204));
    assert_eq!(after.bp, Address::new(0x0082));
    assert_eq!(
        vm.memory().read_cell(Address::new(0x0080)).unwrap(),
        Cell::new(0xaaaa)
    );
    assert_ne!(after.memory, before.memory);
    assert_eq!(after.call_depth, 1);
    assert_eq!(after.step_counter, 3);
    assert!(vm.is_dirty_execution_state());
}

#[test]
fn exit_traps_on_invalid_return_count_and_invalid_frame_metadata() {
    let mut invalid_count_vm = Tbx16Vm::default();
    let mut invalid_count = ImageBuilder::new(CODE_START);
    let exit_xt = invalid_count.primitive(PrimitiveId::Exit);
    let entry_xt = invalid_count.colon_word(0, 0);
    invalid_count.emit_xt(exit_xt);
    invalid_count.emit_cell(Cell::new(2));
    invalid_count.load_into(&mut invalid_count_vm);
    let before = snapshot(&invalid_count_vm);
    assert_eq!(
        invalid_count_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidReturnCount {
            count: Cell::new(2),
        })
    );
    let after = snapshot(&invalid_count_vm);
    assert_eq!(after.ip, Some(Address::new(CODE_START + 10)));
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.rsp, before.rsp);
    assert_eq!(after.bp, before.bp);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.call_depth, before.call_depth);
    assert_eq!(after.step_counter, 2);

    let mut invalid_return_ip_vm = Tbx16Vm::default();
    let mut invalid_return_ip = ImageBuilder::new(CODE_START);
    let halt_xt = invalid_return_ip.primitive(PrimitiveId::Halt);
    let exit_xt = invalid_return_ip.primitive(PrimitiveId::Exit);
    let callee_xt = invalid_return_ip.colon_word(0, 0);
    invalid_return_ip.emit_xt(halt_xt);
    invalid_return_ip.emit_xt(exit_xt);
    invalid_return_ip.emit_cell(Cell::new(0));
    let entry_xt = invalid_return_ip.colon_word(0, 0);
    invalid_return_ip.emit_xt(callee_xt);
    invalid_return_ip.emit_xt(exit_xt);
    invalid_return_ip.emit_cell(Cell::new(0));
    invalid_return_ip.load_into(&mut invalid_return_ip_vm);
    assert_eq!(invalid_return_ip_vm.run(entry_xt), ExecutionOutcome::Halted);
    invalid_return_ip_vm
        .memory_mut()
        .write_cell(DEFAULT_RETURN_STACK_START, Cell::new(0x1235))
        .unwrap();

    assert_eq!(
        invalid_return_ip_vm.run_threaded(Address::new(CODE_START + 16)),
        ExecutionOutcome::Trapped(Tbx16Error::InstructionPointerOutOfRange {
            ip: Address::new(0x1235),
        })
    );
    assert_eq!(invalid_return_ip_vm.step_counter(), 1);

    let mut invalid_caller_bp_vm = Tbx16Vm::default();
    let mut invalid_caller_bp = ImageBuilder::new(CODE_START);
    let halt_xt = invalid_caller_bp.primitive(PrimitiveId::Halt);
    let exit_xt = invalid_caller_bp.primitive(PrimitiveId::Exit);
    let callee_xt = invalid_caller_bp.colon_word(0, 0);
    invalid_caller_bp.emit_xt(halt_xt);
    invalid_caller_bp.emit_xt(exit_xt);
    invalid_caller_bp.emit_cell(Cell::new(0));
    let entry_xt = invalid_caller_bp.colon_word(0, 0);
    invalid_caller_bp.emit_xt(callee_xt);
    invalid_caller_bp.emit_xt(exit_xt);
    invalid_caller_bp.emit_cell(Cell::new(0));
    invalid_caller_bp.load_into(&mut invalid_caller_bp_vm);
    assert_eq!(invalid_caller_bp_vm.run(entry_xt), ExecutionOutcome::Halted);
    invalid_caller_bp_vm
        .memory_mut()
        .write_cell(Address::new(0x0202), Cell::new(0x0081))
        .unwrap();

    assert_eq!(
        invalid_caller_bp_vm.run_threaded(Address::new(CODE_START + 16)),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionState)
    );
    assert_eq!(invalid_caller_bp_vm.step_counter(), 1);
}

#[test]
fn entry_primitive_exit_is_invalid_and_state_preserving() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let exit_xt = image.primitive(PrimitiveId::Exit);
    image.emit_cell(Cell::new(1));
    image.load_into(&mut vm);
    vm.set_instruction_pointer(Address::new(CODE_START + 4))
        .unwrap();
    vm.push_data_cell(Cell::new(0x1111)).unwrap();
    let before = snapshot(&vm);

    assert_eq!(
        vm.run(exit_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidExecutionState)
    );
    let after = snapshot(&vm);
    assert_eq!(after.ip, before.ip);
    assert_eq!(after.dsp, before.dsp);
    assert_eq!(after.rsp, before.rsp);
    assert_eq!(after.bp, before.bp);
    assert_eq!(after.memory, before.memory);
    assert_eq!(after.call_depth, before.call_depth);
    assert_eq!(after.step_counter, 1);
}

#[test]
fn halt_and_trap_leave_dirty_state_until_reset() {
    let mut halt_vm = Tbx16Vm::default();
    let mut halt_image = ImageBuilder::new(CODE_START);
    let halt_xt = halt_image.primitive(PrimitiveId::Halt);
    let entry_xt = halt_image.colon_word(0, 0);
    halt_image.emit_xt(halt_xt);
    halt_image.load_into(&mut halt_vm);

    assert_eq!(halt_vm.run(entry_xt), ExecutionOutcome::Halted);
    assert!(halt_vm.is_dirty_execution_state());
    let halt_snapshot = snapshot(&halt_vm);
    assert_eq!(
        halt_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DirtyExecutionState)
    );
    assert_eq!(snapshot(&halt_vm), halt_snapshot);

    let mut trap_vm = Tbx16Vm::default();
    let mut trap_image = ImageBuilder::new(CODE_START);
    let exit_xt = trap_image.primitive(PrimitiveId::Exit);
    let entry_xt = trap_image.colon_word(0, 0);
    trap_image.emit_xt(exit_xt);
    trap_image.emit_cell(Cell::new(2));
    trap_image.load_into(&mut trap_vm);

    assert_eq!(
        trap_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::InvalidReturnCount {
            count: Cell::new(2),
        })
    );
    assert!(trap_vm.is_dirty_execution_state());
    let trap_snapshot = snapshot(&trap_vm);
    assert_eq!(
        trap_vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DirtyExecutionState)
    );
    assert_eq!(snapshot(&trap_vm), trap_snapshot);

    let mut nested_halt_vm = Tbx16Vm::default();
    let mut nested_halt_image = ImageBuilder::new(CODE_START);
    let halt_xt = nested_halt_image.primitive(PrimitiveId::Halt);
    let callee_xt = nested_halt_image.colon_word(0, 0);
    nested_halt_image.emit_xt(halt_xt);
    let nested_entry_xt = nested_halt_image.colon_word(0, 0);
    nested_halt_image.emit_xt(callee_xt);
    nested_halt_image.load_into(&mut nested_halt_vm);

    assert_eq!(
        nested_halt_vm.run(nested_entry_xt),
        ExecutionOutcome::Halted
    );
    assert_eq!(nested_halt_vm.call_depth(), 1);
    assert_eq!(nested_halt_vm.registers().rsp, Address::new(0x0204));
    assert!(nested_halt_vm.is_dirty_execution_state());
    let nested_halt_snapshot = snapshot(&nested_halt_vm);
    assert_eq!(
        nested_halt_vm.run(nested_entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::DirtyExecutionState)
    );
    assert_eq!(snapshot(&nested_halt_vm), nested_halt_snapshot);

    halt_vm.push_data_cell(Cell::new(0x9999)).unwrap();
    halt_vm.set_step_limit(Some(7));
    halt_vm.reset_execution_state();
    assert_eq!(halt_vm.registers().ip, None);
    assert_eq!(halt_vm.registers().bp, DATA_STACK_START);
    assert_eq!(halt_vm.registers().rsp, DEFAULT_RETURN_STACK_START);
    assert_eq!(halt_vm.call_depth(), 0);
    assert_eq!(halt_vm.step_counter(), 0);
    assert_eq!(halt_vm.registers().dsp, Address::new(0x0082));
    assert_eq!(halt_vm.peek_data_cell(0).unwrap(), Cell::new(0x9999));
    assert!(!halt_vm.is_dirty_execution_state());
    assert_eq!(halt_vm.run(entry_xt), ExecutionOutcome::Halted);
}

#[test]
fn reset_execution_state_preserves_step_limit() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let lit_xt = image.primitive(PrimitiveId::Lit);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    let entry_xt = image.colon_word(0, 0);
    image.emit_xt(lit_xt);
    image.emit_cell(Cell::new(0x1111));
    image.emit_xt(halt_xt);
    image.load_into(&mut vm);

    vm.set_step_limit(Some(1));
    vm.push_data_cell(Cell::new(0x9999)).unwrap();
    vm.reset_execution_state();

    assert_eq!(
        vm.run(entry_xt),
        ExecutionOutcome::Trapped(Tbx16Error::StepLimitExceeded)
    );
    assert_eq!(vm.step_counter(), 1);
    assert_eq!(vm.registers().ip, Some(Address::new(CODE_START + 14)));
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x9999));
}

#[test]
fn reset_execution_state_preserves_output_sink() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let putchr_xt = image.primitive(PrimitiveId::PutChr);
    image.load_into(&mut vm);

    vm.push_data_cell(Cell::new(u16::from(b'Q'))).unwrap();
    assert_eq!(vm.run(putchr_xt), ExecutionOutcome::Returned);

    vm.push_data_cell(Cell::new(0x9999)).unwrap();
    vm.reset_execution_state();

    assert_eq!(vm.output(), b"Q");
    assert_eq!(vm.peek_data_cell(0).unwrap(), Cell::new(0x9999));
}

#[test]
fn primitive_entry_halt_remains_rerunnable_without_reset() {
    let mut vm = Tbx16Vm::default();
    let mut image = ImageBuilder::new(CODE_START);
    let halt_xt = image.primitive(PrimitiveId::Halt);
    image.load_into(&mut vm);

    assert_eq!(vm.run(halt_xt), ExecutionOutcome::Halted);
    assert!(!vm.is_dirty_execution_state());
    assert_eq!(vm.run(halt_xt), ExecutionOutcome::Halted);
    assert_eq!(vm.step_counter(), 1);
}
