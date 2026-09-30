; Standalone ADR #1889 RNG reproduction PoC. This is intentionally not linked
; into vm.s or the private bytecode runtime.
.import _putchar
.export _main, state

.segment "BSS"
state: .res 8
work: .res 8
multiplicand: .res 8
product: .res 8
bound: .res 2
remainder: .res 2
shift_count: .res 1
bit_count: .res 1
bit_value: .res 1
bound_index: .res 1
result: .res 2

.segment "RODATA"
; Test driver replaces this line with each seed's bytes, least significant first.
seed_bytes: .byte $00,$00,$00,$00,$00,$00,$00,$00
bounds: .word 10, 100, 97, 32767, 10
multiplier_bits: .byte $1d,$dd,$6c,$4f,$91,$f4,$45,$25

.segment "CODE"
_main:
    ldx #7
seed_copy:
    lda seed_bytes,x
    sta state,x
    dex
    bpl seed_copy
    lda state
    ora state+1
    ora state+2
    ora state+3
    ora state+4
    ora state+5
    ora state+6
    ora state+7
    bne run_bounds
    lda #$15
    sta state
    lda #$7c
    sta state+1
    lda #$4a
    sta state+2
    lda #$7f
    sta state+3
    lda #$b9
    sta state+4
    lda #$79
    sta state+5
    lda #$37
    sta state+6
    lda #$9e
    sta state+7

run_bounds:
    ldx #0
next_bound:
    lda bounds,x
    sta bound
    lda bounds+1,x
    sta bound+1
    stx bound_index
    jsr next_random
    lda result
    jsr _putchar
    lda result+1
    jsr _putchar
    ldx bound_index
    txa
    clc
    adc #2
    tax
    cpx #10
    bne next_bound
    lda #0
    rts

; ADR #1889 defines xorshift updates followed by wrapping multiplication
; modulo 2^64. Bytes are little-endian; each shift discards overflow bits.
next_random:
    ldx #7
copy_state:
    lda state,x
    sta work,x
    dex
    bpl copy_state
    lda #12
    sta shift_count
right12:
    lsr work+7
    ror work+6
    ror work+5
    ror work+4
    ror work+3
    ror work+2
    ror work+1
    ror work
    dec shift_count
    bne right12
    ldx #0
xor_right12:
    lda state,x
    eor work,x
    sta state,x
    inx
    cpx #8
    bne xor_right12

    ldx #7
copy_state_left:
    lda state,x
    sta work,x
    dex
    bpl copy_state_left
    lda #25
    sta shift_count
left25:
    asl work
    rol work+1
    rol work+2
    rol work+3
    rol work+4
    rol work+5
    rol work+6
    rol work+7
    dec shift_count
    bne left25
    ldx #0
xor_left25:
    lda state,x
    eor work,x
    sta state,x
    inx
    cpx #8
    bne xor_left25

    ldx #7
copy_state_right:
    lda state,x
    sta work,x
    dex
    bpl copy_state_right
    lda #27
    sta shift_count
right27:
    lsr work+7
    ror work+6
    ror work+5
    ror work+4
    ror work+3
    ror work+2
    ror work+1
    ror work
    dec shift_count
    bne right27
    ldx #0
xor_right27:
    lda state,x
    eor work,x
    sta state,x
    inx
    cpx #8
    bne xor_right27

    lda state
    sta multiplicand
    lda state+1
    sta multiplicand+1
    lda state+2
    sta multiplicand+2
    lda state+3
    sta multiplicand+3
    lda state+4
    sta multiplicand+4
    lda state+5
    sta multiplicand+5
    lda state+6
    sta multiplicand+6
    lda state+7
    sta multiplicand+7
    ldx #7
clear_product:
    lda #0
    sta product,x
    dex
    bpl clear_product
    ; Shift-and-add the fixed multiplier; the 8-byte product wraps modulo 2^64.
    ldx #0
multiply_byte:
    lda multiplier_bits,x
    sta bit_value
    ldy #8
multiply_bit:
    lsr bit_value
    bcc multiply_skip_add
    clc
    lda product
    adc multiplicand
    sta product
    lda product+1
    adc multiplicand+1
    sta product+1
    lda product+2
    adc multiplicand+2
    sta product+2
    lda product+3
    adc multiplicand+3
    sta product+3
    lda product+4
    adc multiplicand+4
    sta product+4
    lda product+5
    adc multiplicand+5
    sta product+5
    lda product+6
    adc multiplicand+6
    sta product+6
    lda product+7
    adc multiplicand+7
    sta product+7
multiply_skip_add:
    cpx #7
    bne multiply_shift
    cpy #1
    beq multiply_last_byte
multiply_shift:
    asl multiplicand
    rol multiplicand+1
    rol multiplicand+2
    rol multiplicand+3
    rol multiplicand+4
    rol multiplicand+5
    rol multiplicand+6
    rol multiplicand+7
multiply_last_byte:
    dey
    bne multiply_bit
    inx
    cpx #8
    bne multiply_byte

    lda #0
    sta remainder
    sta remainder+1
    ldx #7
divide_byte:
    lda #8
    sta bit_count
divide_bit:
    asl product,x
    rol remainder
    rol remainder+1
    lda remainder+1
    cmp bound+1
    bcc no_subtract
    bne subtract_bound
    lda remainder
    cmp bound
    bcc no_subtract
subtract_bound:
    sec
    lda remainder
    sbc bound
    sta remainder
    lda remainder+1
    sbc bound+1
    sta remainder+1
no_subtract:
    dec bit_count
    bne divide_bit
    dex
    bpl divide_byte
    clc
    lda remainder
    adc #1
    sta result
    lda remainder+1
    adc #0
    sta result+1
    rts
