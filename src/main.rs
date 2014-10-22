#![feature(phase)]
#[phase(plugin, link)] extern crate log;

const U4_MASK: u8 = 0b00001111; // Takes the lower 4 bits of a byte. Useful for decoding & encoding nibbles.

enum CpuState {
	Continue,
	Halt,
}

struct P150Cpu {
	ip:  u8,
	ir: u16,

	reg: [u8, ..16],
	mem: [u8, ..256],
}

impl P150Cpu {
	fn new() -> P150Cpu {
		P150Cpu {
			ip:  0x0,
			ir:  0x00,

			reg: [0u8, ..16],
			mem: [0u8, ..256],
		}
	}

	fn dump(&self) {
		println!("IP: {}, IR: {}", self.ip, self.ir);

		println!("Registers")
		for (addr, cell) in self.reg.iter().enumerate() {
			println!("{}: {}", addr, cell)
		}
	}

	// Read an array of instructions into memory
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

	fn tick(&mut self) -> CpuState {
		self.fetch();

		// decode
		let op = (self.ir >> 12) as u8;
		match op {
			0x9   => { // RSET
				let rloc = ((self.ir >> 8) as u8) & U4_MASK; // lower 4 bytes of first byte
				let rval = self.ir as u8;                    // value is entire second byte
				self.reg[rloc as uint] = rval;               // store value in register

				Continue
			},

			0xB   => { // HALT
				Halt 
			},
			_     => { debug!("read opcode: {}", op); Continue },
		}
	}

	fn fetch(&mut self) {
		// load PC -> IR
		let byte_1 = self.mem[(self.ip+0) as uint];
		let byte_2 = self.mem[(self.ip+1) as uint];
		
		self.ir  = (byte_1 as u16 << 8) | (byte_2 as u16);
		self.ip += 2;
		
		debug!("IR set to {} ({},{})", self.ir, byte_1, byte_2)
	}
}

fn main() {
	let program = [0x911E, 0x920C, 0xB000];
	let mut cpu = P150Cpu::new();
	cpu.init_mem(program);

	loop {
		match cpu.tick() {
			Continue => { continue; },
			Halt => { println!("CPU halted."); cpu.dump(); break; },
		}
	}
}

