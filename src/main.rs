#![feature(phase)]
#[phase(plugin, link)] extern crate log;

const U4_LOW:  u8 = 0b00001111;
const U4_HIGH: u8 = 0b11110000;

enum CpuState {
	Continue,
	Halt,
}

/// The P150 virtual machine
struct P150Cpu {
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
	/// Programs must start at memory address 0x00
	fn new() -> P150Cpu {
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
	fn dump(&self) {
		println!("IP: 0x{:02X}, IR: 0x{:04X}", self.ip, self.ir);

		println!("Registers")
		for (addr, cell) in self.reg.iter().enumerate() {
			println!("{}: {}", addr, cell)
		}
	}

	/// Read an array of instructions into memory
	/// This reads two bytes at a time from the `memory` array
	/// and loads them into the P150s RAM bank, starting from address 0.
	fn init_mem(&mut self, memory: &[u16]) {
		let mut next_cell = 0x00;

		for op in memory.iter() {
			let byte_1 = (*op >> 8) as u8;
			let byte_2 = *op as u8;

			self.mem[next_cell]     = byte_1;
			self.mem[next_cell + 1] = byte_2;
			next_cell += 2
		}
	}

	/// Runs the entire fetch, execute, decode cycle against the current IP
	fn tick(&mut self) -> CpuState {
		self.fetch();

		// decode
		let op = (self.ir >> 12) as u8;
		match op {
			0x0   => { // ADDB
				let rloc_i0 = l_nibble((self.ir >> 8) as u8);   // first input: lower nibble of first byte
				let rloc_i1 = h_nibble(self.ir as u8);          // second input: upper nibble of second byte
				let rloc_o0 = l_nibble(self.ir as u8);          // output: lower nibble of second byte

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] + self.reg[rloc_i1 as uint];
				Continue
			},

			0x3   => { // AND
				let rloc_i0 = l_nibble((self.ir >> 8) as u8);
				let rloc_i1 = h_nibble(self.ir as u8);
				let rloc_o0 = l_nibble(self.ir as u8);

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] & self.reg[rloc_i1 as uint];
				Continue
			},

			0x4   => { // OR
				let rloc_i0 = l_nibble((self.ir >> 8) as u8);
				let rloc_i1 = h_nibble(self.ir as u8);
				let rloc_o0 = l_nibble(self.ir as u8);

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] | self.reg[rloc_i1 as uint];
				Continue
			},

			0x5   => { // XOR
				let rloc_i0 = l_nibble((self.ir >> 8) as u8);
				let rloc_i1 = h_nibble(self.ir as u8);
				let rloc_o0 = l_nibble(self.ir as u8);

				self.reg[rloc_o0 as uint] = self.reg[rloc_i0 as uint] ^ self.reg[rloc_i1 as uint];
				Continue
			},

			0x6   => { // MLOAD
				let rloc_o0 = l_nibble((self.ir >> 8) as u8);
				let mloc_i0 = self.ir as u8;

				self.reg[rloc_o0 as uint] = self.mem[mloc_i0 as uint];
				Continue
			},

			0x7   => { // MSTOR
				let rloc_i0 = l_nibble((self.ir >> 8) as u8);
				let mloc_o0 = self.ir as u8;
				
				self.mem[mloc_o0 as uint] = self.reg[rloc_i0 as uint];
				Continue
			},

			0x9   => { // RSET
				let rloc = ((self.ir >> 8) as u8) & U4_LOW;  // lower nibble of first byte
				let rval = self.ir as u8;                    // value is entire second byte
				self.reg[rloc as uint] = rval;               // store value in register

				Continue
			},

			0xB   => { Halt },
			_     => { debug!("read unhandled opcode: {}", op); Halt },
		}
	}

	/// Load the instruction at `IP` and advance the pointer by two bytes.
	/// The instruction is packed into a single `u16` and stored in the instruction register.
	fn fetch(&mut self) {
		// load PC -> IR
		let byte_1 = self.mem[(self.ip+0) as uint];
		let byte_2 = self.mem[(self.ip+1) as uint];
		
		self.ir  = (byte_1 as u16 << 8) | (byte_2 as u16);
		self.ip += 2;
		
		debug!("IR set to 0x{:04X} ({:02X},{:02X})", self.ir, byte_1, byte_2)
	}
}

/// Take the lower nibble of a byte
fn l_nibble(byte: u8) -> u8 {
	(byte & U4_LOW)
}

/// Take the upper nibble of a byte and shift it
/// towards the least significant bits.
fn h_nibble(byte: u8) -> u8 {
	(byte & U4_HIGH) >> 4
}

fn main() {
	let mut cpu = P150Cpu::new();
	let program = [0x911E, 0x920C, 0x0123, 0x73F0, 0x61F0, 0x0123, 0x73F1, 0xB000];
	cpu.init_mem(program);
	
	// 0x911E => 0x9 (rset), 0x1 (reg), 0x1E (val: 30)
	// 0x920C => 0x9 (rset), 0x2 (reg), 0x0C (val: 12)
	// 0x0123 => 0x0 (addb), 0x1 (in1), 0x2 (in2), 0x3 (out1)
	// 0x73F0 => 0x7 (msto), 0x3 (reg), 0xF0 (mem)
	// 0x61F0 => 0x7 (mlod), 0x1 (reg), 0xF0 (mem)
	// 0x0123 => 0x0 (addb), 0x1 (in1), 0x2 (in2), 0x3 (out1)
	// 0x73F1 => 0x7 (msto), 0x3 (reg), 0xF1 (mem)

	loop {
		match cpu.tick() {
			Continue => { continue; },
			Halt => { println!("CPU halted."); cpu.dump(); break; },
		}
	}
}

