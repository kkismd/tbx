use tbx::tbx16::address::Address;
use tbx::tbx16::cell::Cell;
use tbx::tbx16::error::Tbx16Error;
use tbx::tbx16::memory::MEMORY_SIZE;
use tbx::tbx16::stack::{ReturnFrame, StackRegion};
use tbx::tbx16::{
    Tbx16Vm, DATA_STACK_END, DATA_STACK_START, DEFAULT_RETURN_STACK_END, DEFAULT_RETURN_STACK_START,
};

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
