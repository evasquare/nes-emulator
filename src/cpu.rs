use crate::opcodes;
use std::collections::HashMap;

bitflags! {
    /// # Status Register (P) http://wiki.nesdev.com/w/index.php/Status_flags
    ///
    ///  7 6 5 4 3 2 1 0
    ///  N V _ B D I Z C
    ///  | |   | | | | +--- Carry Flag
    ///  | |   | | | +----- Zero Flag
    ///  | |   | | +------- Interrupt Disable
    ///  | |   | +--------- Decimal Mode (not used on NES)
    ///  | |   +----------- Break Command
    ///  | +--------------- Overflow Flag
    ///  +----------------- Negative Flag
    ///
    pub struct CpuFlags: u8 {
        const CARRY             = 0b00000001;
        const ZERO              = 0b00000010;
        const INTERRUPT_DISABLE = 0b00000100;
        const DECIMAL_MODE      = 0b00001000;
        const BREAK             = 0b00010000;
        const BREAK2            = 0b00100000;
        const OVERFLOW          = 0b01000000;
        const NEGATIV           = 0b10000000;
    }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

pub struct CPU {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: CpuFlags,
    pub program_counter: u16,
    pub stack_pointer: u8,
    memory: [u8; 0xFFFF],
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPage_X,
    ZeroPage_Y,
    Absolute,
    Absolute_X,
    Absolute_Y,
    Indirect_X,
    Indirect_Y,
    NoneAddressing,
}

trait Mem {
    fn mem_read(&self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

impl Mem for CPU {
    fn mem_read(&self, addr: u16) -> u8 {
        self.memory[addr as usize]
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.memory[addr as usize] = data;
    }
}

impl CPU {
    pub fn new() -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            stack_pointer: STACK_RESET,
            program_counter: 0,
            status: CpuFlags::from_bits_truncate(0b100100),
            memory: [0; 0xFFFF],
        }
    }

    fn get_operand_address(&self, mode: &AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => self.program_counter,

            AddressingMode::ZeroPage => self.mem_read(self.program_counter) as u16,

            AddressingMode::Absolute => self.mem_read_u16(self.program_counter),

            AddressingMode::ZeroPage_X => {
                let pos = self.mem_read(self.program_counter);
                let addr = pos.wrapping_add(self.register_x) as u16;
                addr
            }
            AddressingMode::ZeroPage_Y => {
                let pos = self.mem_read(self.program_counter);
                let addr = pos.wrapping_add(self.register_y) as u16;
                addr
            }

            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(self.program_counter);
                let addr = base.wrapping_add(self.register_x as u16);
                addr
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(self.program_counter);
                let addr = base.wrapping_add(self.register_y as u16);
                addr
            }

            AddressingMode::Indirect_X => {
                let base = self.mem_read(self.program_counter);

                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                (hi as u16) << 8 | (lo as u16)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(self.program_counter);

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);
                let deref = deref_base.wrapping_add(self.register_y as u16);
                deref
            }

            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }

    /** ## LDY - Load Y Register
     * Loads a byte of memory into the Y register
     * setting the zero and negative flags as appropriate.
     */
    fn ldy(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_y = data;
        self.update_zero_and_negative_flags(self.register_y);
    }

    /** ## LDX - Load X Register
     * Loads a byte of memory into the X register
     * setting the zero and negative flags as appropriate.
     */
    fn ldx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_x = data;
        self.update_zero_and_negative_flags(self.register_x);
    }

    /** ## LDA - Load Accumulator
     * Loads a byte of memory into the accumulator
     * setting the zero and negative flags as appropriate.
     */
    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let value: u8 = self.mem_read(addr);
        self.set_register_a(value);
    }

    /** ## STA - Store Accumulator
     * Stores the contents of the accumulator into memory.
     */
    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn set_register_a(&mut self, value: u8) {
        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
    }

    /**
     * ## AND - Logical AND
     * A logical AND is performed, bit by bit, on the accumulator
     * contents using the contents of a byte of memory.
     */
    fn and(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data & self.register_a);
    }

    /**
     * ## EOR - Exclusive OR
     * An exclusive OR is performed, bit by bit, on the accumulator
     * contents using the contents of a byte of memory.
     */
    fn eor(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data ^ self.register_a);
    }

    /**
     * ## ORA - Logical Inclusive OR
     * An inclusive OR is performed, bit by bit, on the accumulator
     * contents using the contents of a byte of memory.
     */
    fn ora(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data | self.register_a);
    }

    /**
     * ## TAX - Transfer Accumulator to X
     * Copies the current contents of the accumulator into the
     * X register and sets the zero and negative flags as appropriate.
     */
    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        if result & 0b1000_0000 != 0 {
            self.status.insert(CpuFlags::NEGATIV);
        } else {
            self.status.remove(CpuFlags::NEGATIV);
        }
    }

    /**
     * ## INX - Increment X Register
     * Adds one to the X register setting the zero and
     * negative flags as appropriate.
     */
    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    /**
     * ## INY - Increment Y Register
     * Adds one to the Y register setting the zero and
     * negative flags as appropriate.
     */
    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    pub fn load_and_run(&mut self, program: Vec<u8>) {
        self.load(program);
        self.program_counter = self.mem_read_u16(0xFFFC);
        self.run()
    }

    pub fn load(&mut self, program: Vec<u8>) {
        self.memory[0x8000..(0x8000 + program.len())].copy_from_slice(&program[..]);
        self.mem_write_u16(0xFFFC, 0x8000);
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.stack_pointer = STACK_RESET;
        self.status = CpuFlags::from_bits_truncate(0b100100);

        self.program_counter = self.mem_read_u16(0xFFFC);
    }

    fn set_carry_flag(&mut self) {
        self.status.insert(CpuFlags::CARRY)
    }

    fn clear_carry_flag(&mut self) {
        self.status.remove(CpuFlags::CARRY)
    }

    /// note: ignoring decimal mode
    /// http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
    fn add_to_register_a(&mut self, data: u8) {
        let sum = self.register_a as u16
            + data as u16
            + (if self.status.contains(CpuFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry: bool = sum > 0xff;

        if carry {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.register_a) & 0x80 != 0 {
            self.status.insert(CpuFlags::OVERFLOW);
        } else {
            self.status.remove(CpuFlags::OVERFLOW)
        }

        self.set_register_a(result);
    }

    /**
     * ## SBC - Subtract with Carry
     * This instruction subtracts the contents of
     * a memory location to the accumulator together
     * with the not of the carry bit. If overflow occurs
     * the carry bit is clear, this enables multiple byte
     * subtraction to be performed.
     */
    fn sbc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let data = self.mem_read(addr);
        self.add_to_register_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
    }

    /**
     * ## ADC - Add with Carry
     * This instruction adds the contents of
     * a memory location to the accumulator together
     * with the carry bit. If overflow occurs the carry bit is set,
     * this enables multiple byte addition to be performed.
     */
    fn adc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);
        self.add_to_register_a(value);
    }

    /** Stack pointer increases as the user pops the stack. */
    fn stack_pop(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.mem_read((STACK as u16) + self.stack_pointer as u16)
    }

    /** Stack pointer decreases as the user pushes to the stack. */
    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.stack_pointer as u16, data);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1)
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;

        /*
        1. `hi << 8` shifts the high byte 8 bits
           to the left, making space for the low byte.

        2. `| lo`` combines the shifted hi and lo using
           a bitwise OR, reconstructing the original 16-bit value.
         */
        hi << 8 | lo
    }

    fn asl_accumulator(&mut self) {
        let mut data = self.register_a;
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        self.set_register_a(data)
    }

    /**
     * ## ASL - Arithmetic Shift Left
     * This operation shifts all the bits of the accumulator
     * or memory contents one bit left. Bit 0 is set to 0 and
     * bit 7 is placed in the carry flag. The effect of this operation is
     * to multiply the memory contents by 2 (ignoring 2's complement
     * considerations), setting the carry if the result will
     * not fit in 8 bits.
     */
    fn asl(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn lsr_accumulator(&mut self) {
        let mut data = self.register_a;
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.set_register_a(data)
    }

    /**
     * ## LSR - Logical Shift Right
     * Each of the bits in A or M is shift one place to the right.
     * The bit that was in bit 0 is shifted into the carry flag.
     * Bit 7 is set to zero.
     */
    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    /**
     * ## ROL - Rotate Left
     * Move each of the bits in either A or M one place to the left.
     * Bit 0 is filled with the current value of the carry flag whilst
     * the old bit 7 becomes the new carry flag value.
     */
    fn rol(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        if old_carry {
            data = data | 1;
        }
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn rol_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        if old_carry {
            data = data | 1;
        }
        self.set_register_a(data);
    }

    /**
     * ## ROR - Rotate Right
     * Move each of the bits in either A or M one place to the right.
     * Bit 7 is filled with the current value of the carry flag
     * whilst the old bit 0 becomes the new carry flag value.
     */
    fn ror(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        if old_carry {
            data = data | 0b10000000;
        }
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn ror_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        if old_carry {
            data = data | 0b10000000;
        }
        self.set_register_a(data);
    }

    /**
     * ## INC - Increment Memory
     * Adds one to the value held at a specified memory location
     * setting the zero and negative flags as appropriate.
     */
    fn inc(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    /**
     * ## DEY - Decrement Y Register
     * Subtracts one from the Y register
     * setting the zero and negative flags as appropriate.
     */
    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    /** ## DEX - Decrement X Register
     * Subtracts one from the X register
     * setting the zero and negative flags as appropriate.
     */
    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    /** ## DEC - Decrement Memory
     * Subtracts one from the value held at a specified memory
     * location setting the zero and negative flags as appropriate.
     */
    fn dec(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    /** ## PLA - Pull Accumulator
     * Pulls an 8 bit value from the stack and into the accumulator.
     * The zero and negative flags are set as appropriate.
     */
    fn pla(&mut self) {
        let data = self.stack_pop();
        self.set_register_a(data);
    }

    /** ## PLP - Pull Processor Status
     * Pulls an 8 bit value from the stack and into the processor flags
     * The flags will take on new states as determined by the value pulled.
     */
    fn plp(&mut self) {
        self.status.bits = self.stack_pop();
        self.status.remove(CpuFlags::BREAK);
        self.status.insert(CpuFlags::BREAK2);
    }

    /** ## PHP - Push Processor Status
     * Pushes a copy of the status flags on to the stack.
     */
    fn php(&mut self) {
        //http://wiki.nesdev.com/w/index.php/CPU_status_flag_behavior
        let mut flags = self.status.clone();
        flags.insert(CpuFlags::BREAK);
        flags.insert(CpuFlags::BREAK2);
        self.stack_push(flags.bits());
    }
    /**
     * ## BIT - Bit Test
     * This instructions is used to test if one or more bits are
     * set in a target memory location. The mask pattern in A is
     * ANDed with the value in memory to set or clear the zero flag,
     * but the result is not kept. Bits 7 and 6 of the value from
     * memory are copied into the N and V flags.
     */
    fn bit(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        let and = self.register_a & data;
        if and == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        self.status.set(CpuFlags::NEGATIV, data & 0b10000000 > 0);
        self.status.set(CpuFlags::OVERFLOW, data & 0b01000000 > 0);
    }

    /**
     * ## CMP - Compare
     * This instruction compares the contents of the accumulator
     * with another memory held value and sets the zero
     * and carry flags as appropriate.
     */
    fn compare(&mut self, mode: &AddressingMode, compare_with: u8) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        if data <= compare_with {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        self.update_zero_and_negative_flags(compare_with.wrapping_sub(data));
    }

    fn branch(&mut self, condition: bool) {
        if condition {
            let jump: i8 = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add(jump as u16);

            self.program_counter = jump_addr;
        }
    }

    pub fn run(&mut self) {
        let ref opcodes: HashMap<u8, &'static opcodes::OpCode> = *opcodes::OPCODES_MAP;

        loop {
            let code = self.mem_read(self.program_counter);
            self.program_counter += 1;
            let program_counter_state = self.program_counter;

            let opcode = opcodes
                .get(&code)
                .expect(&format!("OpCode {:x} is not recognized", code));

            match code {
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                }

                0xAA => self.tax(),
                0xe8 => self.inx(),
                0x00 => return,

                /* CLD */ 0xd8 => self.status.remove(CpuFlags::DECIMAL_MODE),

                /* CLI */ 0x58 => self.status.remove(CpuFlags::INTERRUPT_DISABLE),

                /* CLV */ 0xb8 => self.status.remove(CpuFlags::OVERFLOW),

                /* CLC */ 0x18 => self.clear_carry_flag(),

                /* SEC */ 0x38 => self.set_carry_flag(),

                /* SEI */ 0x78 => self.status.insert(CpuFlags::INTERRUPT_DISABLE),

                /* SED */ 0xf8 => self.status.insert(CpuFlags::DECIMAL_MODE),

                /* PHA */ 0x48 => self.stack_push(self.register_a),

                /* PLA */
                0x68 => {
                    self.pla();
                }

                /* PHP */
                0x08 => {
                    self.php();
                }

                /* PLP */
                0x28 => {
                    self.plp();
                }

                /* ADC */
                0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                }

                /* SBC */
                0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
                    self.sbc(&opcode.mode);
                }

                /* AND */
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                }

                /* EOR */
                0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                    self.eor(&opcode.mode);
                }

                /* ORA */
                0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
                    self.ora(&opcode.mode);
                }

                /* LSR */ 0x4a => self.lsr_accumulator(),

                /* LSR */
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }

                /*ASL*/ 0x0a => self.asl_accumulator(),

                /* ASL */
                0x06 | 0x16 | 0x0e | 0x1e => {
                    self.asl(&opcode.mode);
                }

                /*ROL*/ 0x2a => self.rol_accumulator(),

                /* ROL */
                0x26 | 0x36 | 0x2e | 0x3e => {
                    self.rol(&opcode.mode);
                }

                /* ROR */ 0x6a => self.ror_accumulator(),

                /* ROR */
                0x66 | 0x76 | 0x6e | 0x7e => {
                    self.ror(&opcode.mode);
                }

                /* INC */
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }

                /* INY */
                0xc8 => self.iny(),

                /* DEC */
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }

                /* DEX */
                0xca => {
                    self.dex();
                }

                /* DEY */
                0x88 => {
                    self.dey();
                }

                /* CMP */
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.compare(&opcode.mode, self.register_a);
                }

                /* CPY */
                0xc0 | 0xc4 | 0xcc => {
                    self.compare(&opcode.mode, self.register_y);
                }

                /* CPX */
                0xe0 | 0xe4 | 0xec => self.compare(&opcode.mode, self.register_x),

                /* JMP Absolute */
                0x4c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = mem_address;
                }

                /* JMP Indirect */
                0x6c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    // let indirect_ref = self.mem_read_u16(mem_address);
                    //6502 bug mode with with page boundary:
                    //  if address $3000 contains $40, $30FF contains $80, and $3100 contains $50,
                    // the result of JMP ($30FF) will be a transfer of control to $4080 rather than $5080 as you intended
                    // i.e. the 6502 took the low byte of the address from $30FF and the high byte from $3000

                    let indirect_ref = if mem_address & 0x00FF == 0x00FF {
                        let lo = self.mem_read(mem_address);
                        let hi = self.mem_read(mem_address & 0xFF00);
                        (hi as u16) << 8 | (lo as u16)
                    } else {
                        self.mem_read_u16(mem_address)
                    };

                    self.program_counter = indirect_ref;
                }

                /* JSR */
                0x20 => {
                    self.stack_push_u16(self.program_counter + 2 - 1);
                    let target_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = target_address
                }

                /* RTS */
                0x60 => {
                    self.program_counter = self.stack_pop_u16() + 1;
                }

                /* RTI */
                0x40 => {
                    self.status.bits = self.stack_pop();
                    self.status.remove(CpuFlags::BREAK);
                    self.status.insert(CpuFlags::BREAK2);

                    self.program_counter = self.stack_pop_u16();
                }

                /* BNE */
                0xd0 => {
                    self.branch(!self.status.contains(CpuFlags::ZERO));
                }

                /* BVS */
                0x70 => {
                    self.branch(self.status.contains(CpuFlags::OVERFLOW));
                }

                /* BVC */
                0x50 => {
                    self.branch(!self.status.contains(CpuFlags::OVERFLOW));
                }

                /* BPL */
                0x10 => {
                    self.branch(!self.status.contains(CpuFlags::NEGATIV));
                }

                /* BMI */
                0x30 => {
                    self.branch(self.status.contains(CpuFlags::NEGATIV));
                }

                /* BEQ */
                0xf0 => {
                    self.branch(self.status.contains(CpuFlags::ZERO));
                }

                /* BCS */
                0xb0 => {
                    self.branch(self.status.contains(CpuFlags::CARRY));
                }

                /* BCC */
                0x90 => {
                    self.branch(!self.status.contains(CpuFlags::CARRY));
                }

                /* BIT */
                0x24 | 0x2c => {
                    self.bit(&opcode.mode);
                }

                /* STA */
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                /* STX */
                0x86 | 0x96 | 0x8e => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_x);
                }

                /* STY */
                0x84 | 0x94 | 0x8c => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_y);
                }

                /* LDX */
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    self.ldx(&opcode.mode);
                }

                /* LDY */
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    self.ldy(&opcode.mode);
                }

                /* NOP */
                0xea => {
                    //do nothing
                }

                /* TAY */
                0xa8 => {
                    self.register_y = self.register_a;
                    self.update_zero_and_negative_flags(self.register_y);
                }

                /* TSX */
                0xba => {
                    self.register_x = self.stack_pointer;
                    self.update_zero_and_negative_flags(self.register_x);
                }

                /* TXA */
                0x8a => {
                    self.register_a = self.register_x;
                    self.update_zero_and_negative_flags(self.register_a);
                }

                /* TXS */
                0x9a => {
                    self.stack_pointer = self.register_x;
                }

                /* TYA */
                0x98 => {
                    self.register_a = self.register_y;
                    self.update_zero_and_negative_flags(self.register_a);
                }

                _ => todo!(),
            }

            if program_counter_state == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let mut cpu = CPU::new();

        cpu.load_and_run(vec![0xa9, 0x05, 0x00]);

        assert_eq!(cpu.register_a, 5);
        assert!(cpu.status.bits() & 0b0000_0010 == 0b00);
        assert!(cpu.status.bits() & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xaa_tax_move_a_to_x() {
        let mut cpu = CPU::new();
        cpu.register_a = 10;

        cpu.load_and_run(vec![0xaa, 0x00]);

        assert_eq!(cpu.register_x, 10)
    }

    #[test]
    fn test_5_ops_working_together() {
        let mut cpu = CPU::new();

        cpu.load_and_run(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 0xc1)
    }

    #[test]
    fn test_inx_overflow() {
        let mut cpu = CPU::new();
        cpu.register_x = 0xff;

        cpu.load_and_run(vec![0xe8, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 1)
    }

    #[test]
    fn test_lda_from_memory() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);

        cpu.load_and_run(vec![0xa5, 0x10, 0x00]);

        assert_eq!(cpu.register_a, 0x55);
    }
}
