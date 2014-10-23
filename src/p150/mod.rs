// masks for pulling nibbles out of bytes
const BYTE_WIDTH: uint = 8;
const U4_LOW:     u8   = 0b00001111;
const U4_HIGH:    u8   = 0b11110000;


/// If the CPU enters the `Halt` state then any additional ticks will result in
/// the program exhibiting undefined behavior.
///
#[deriving(PartialEq,Show)]
pub enum CpuState {
	Continue,
	Halt,
}

/// The P150 virtual machine
/// Represents all information the CPU needs to continue executing a
/// program stored in its main memory.
pub struct P150Cpu {
	ip:  u8,
	ir: u16,

	reg: [u8, ..16],
	mem: [u8, ..256],
}

impl P150Cpu {
	/// Initializes the P150 CPU
	///
	/// NOTE: This implementation 0s memory; but this is not guaranteed
	/// by the machine specification.
	///
	/// Programs MUST start at memory address 0x00
	pub fn new() -> P150Cpu {
		P150Cpu {
			ip:  0x00,
			ir:  0x0000,

			reg: [0u8, ..16],
			mem: [0u8, ..256],
		}
	}

	/// Prints the current machine state to the console window
	/// This includes the IP, IR, and all registers.
	/// (Registers will be formatted as 2s complement numbers.)
	pub fn dump(&self) {
		println!("IP: 0x{:02X}, IR: 0x{:04X}", self.ip, self.ir);

		println!("---\nRegisters\n---")
		for (addr, cell) in self.reg.iter().enumerate() {
			println!("{:01X}: {}", addr, cell)
		}

		println!("---")

		debug!("Memory")
		for (addr, cell) in self.mem.iter().enumerate() {
			debug!("{:02X}: {}", addr, cell)
		}
	}

	#[cfg(test)]
	fn get_reg(&self) -> &[u8] { self.reg }

	/// Read an array of instructions into main memory
	/// This reads two bytes at a time from the `memory` array
	/// and loads them into the P150s RAM bank, starting from address 0.
	pub fn init_mem(&mut self, memory: &[u16]) {
		assert!(memory.len() < (256 / 2)); // program cannot be larger than memory
		let mut next_cell = 0x00;

		// zero memory
		self.mem = [0, ..256];
		for op in memory.iter() {
			let byte_1 = (*op >> 8) as u8;
			let byte_2 = *op as u8;

			self.mem[next_cell]     = byte_1;
			self.mem[next_cell + 1] = byte_2;
			next_cell += 2
		}
	}

	/// Runs the entire fetch, execute, decode cycle against the current IP
	pub fn tick(&mut self) -> CpuState {
		self.fetch();

		// decode & execute
		let op = (self.ir >> 12) as u8;
		match op {
			0x0   => { // MLOAD
				let rloc_o0 = lo_nibble((self.ir >> 8) as u8);
				let mloc_i0 = self.ir as u8;

				self.reg[rloc_o0 as uint] = self.mem[mloc_i0 as uint];
				Continue
			},

			0x1   => { // RSET
				let rloc = lo_nibble((self.ir >> 8) as u8);  // lower nibble of first byte
				let rval = self.ir as u8;                    // value is entire second byte

				self.reg[rloc as uint] = rval;               // store value in register
				Continue
			},
			
			0x2   => { // MSTOR
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);
				let mloc_o0 = self.ir as u8;

				self.mem[mloc_o0 as uint] = self.reg[rloc_i0 as uint];
				Continue
			},

			0x3   => { // RMOV
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);
				let rloc_o0 = hi_nibble(self.ir as u8);

				debug!("moving from {:02X} to {:02X}", rloc_i0, rloc_o0)
				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint];
				Continue
			},

			0x4   => { // ADDB
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);   // first input: lower nibble of first byte
				let rloc_i1 = hi_nibble(self.ir as u8);          // second input: upper nibble of second byte
				let rloc_o0 = lo_nibble(self.ir as u8);          // output: lower nibble of second byte

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] + self.reg[rloc_i1 as uint];
				Continue
			},

			0x6   => { // OR
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);
				let rloc_i1 = hi_nibble(self.ir as u8);
				let rloc_o0 = lo_nibble(self.ir as u8);

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] | self.reg[rloc_i1 as uint];
				Continue
			},

			0x7   => { // AND
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);
				let rloc_i1 = hi_nibble(self.ir as u8);
				let rloc_o0 = lo_nibble(self.ir as u8);

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] & self.reg[rloc_i1 as uint];
				Continue
			},

			0x8   => { // XOR
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);
				let rloc_i1 = hi_nibble(self.ir as u8);
				let rloc_o0 = lo_nibble(self.ir as u8);

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] ^ self.reg[rloc_i1 as uint];
				Continue
			},

			0x9   => { // ROT
				// LHS shifts <width> bits off the (left) end of the bitstring
				// RHS shifts the bitstring to the right until only the bits which fell off remain.
				//   LHS is the remaining MSB bits; RHS is the remaining LSB bits
				//   ∴ LHS <OR> RHS provides a rotated bitstring
				//
				let rloc_i0   = lo_nibble((self.ir >> 8) as u8) as uint;          // register is first nibble ...
				let swidth    = (hi_nibble(self.ir as u8) & 0b0000_0111) as uint; // last three bytes of second nibble ...
				self.reg[rloc_i0 as uint] = (self.reg[rloc_i0] << swidth) | (self.reg[rloc_i0] >> (BYTE_WIDTH - swidth));

				Continue
			},

			0xA   => { // JMPEQ
				let rloc_i0 = lo_nibble((self.ir >> 8) as u8);
				let next_ip = self.ir as u8;

				if self.reg[rloc_i0 as uint] == self.reg[0] { self.ip = next_ip }
				Continue
			},

			0xB   => { Halt },
			_     => { debug!("halt, cpu on fire: {}", op); Halt },
		}
	}

	/// Load the instruction at `IP` and advance the pointer by two bytes.
	/// The instruction is packed into a single `u16` and stored in the instruction register.
	fn fetch(&mut self) {
		// fetch two bytes from PC
		let byte_1 = self.mem[(self.ip+0) as uint];
		let byte_2 = self.mem[(self.ip+1) as uint];

		self.ir  = (byte_1 as u16 << 8) | (byte_2 as u16); // load byets into IR
		self.ip += 2;                                      // increment instruction pointer

		debug!("IR set to 0x{:04X} ({:02X},{:02X})", self.ir, byte_1, byte_2)
	}
}

/// Take the lower nibble of a byte
fn lo_nibble(byte: u8) -> u8 {
	(byte & U4_LOW)
}

/// Take the upper nibble of a byte and shift it
/// towards the least significant bits.
fn hi_nibble(byte: u8) -> u8 {
	(byte & U4_HIGH) >> 4
}

#[test]
fn test_hammer_time() {
	// cpu should stop after 1 tick of this program
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0xB000]);

	assert_eq!(cpu.tick(), Halt)
}

#[test]
fn test_registers() {
	// cpu should set, move registers accordingly
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0x1110, 0x3130, 0xB000]);
	boot(&mut cpu);

	assert_eq!(cpu.get_reg()[0x3], 16)
}

#[test]
fn test_memory() {
	// memory sets and memory stores should read back successfully
	// uninitialized registers should not match initialized registers
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0x1120, 0x1330, 0x2140, 0x0240, 0xB000]);
	boot(&mut cpu);

	assert!(cpu.get_reg()[0x1] == cpu.get_reg()[0x2]);
	assert!(cpu.get_reg()[0x1] != cpu.get_reg()[0x3]);
}

#[test]
fn test_bin() {
	// tests the various binary operations against their rustc counterparts.
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0x1121, 0x1222, 0x6123, 0x7124, 0x8125, 0xB000]);
	boot(&mut cpu);

	assert!(cpu.get_reg()[0x3] == (0x21 | 0x22));
	assert!(cpu.get_reg()[0x4] == (0x21 & 0x22));
	assert!(cpu.get_reg()[0x5] == (0x21 ^ 0x22));
}

#[test]
fn test_math() {
	// tests basic 2s comp. addition
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0x1120, 0x120A, 0x4123, 0xB000]);
	boot(&mut cpu);

	assert_eq!(cpu.get_reg()[0x3], cpu.get_reg()[0x1] + cpu.get_reg()[0x2]);
	assert_eq!(cpu.get_reg()[0x3], 0x20 + 0x0A);
}

#[test]
fn test_branch() {
	// checks that the program branches; skipping a halt and setting a status register
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0x1021, 0x1121, 0xA108, 0xB000, 0x122A, 0xB000]);
	boot(&mut cpu);

	assert_eq!(cpu.get_reg()[0x2], 0x2A)
}

#[test]
fn test_shift() {
	// checks that the program rotates a single nibble 4 places
	// this should move a single hex digit from the lhs to the rhs.
	//
	// NOTE: shifting 12 bits (12 - 8) and 4 bits should be equivalent.
	let mut cpu = P150Cpu::new();
	cpu.init_mem([0x10B0, 0x11B0, 0x90C0, 0x9140, 0xB000]);
	boot(&mut cpu);

	assert_eq!(cpu.get_reg()[0x0], 0x0B);
	assert_eq!(cpu.get_reg()[0x1], 0x0B);
}

#[cfg(test)]
fn boot(cpu: &mut P150Cpu) {
	loop {
		if cpu.tick() == Halt { break; }
	}
}

