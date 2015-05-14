#[macro_use] extern crate log;
#[macro_use] extern crate nickel;
extern crate plugin;
extern crate rustc_serialize;
extern crate typemap;

use p150::{CpuState, P150Cpu};

use std::collections::HashMap;
use std::sync::{Arc,RwLock};

use nickel::{JsonBody, Nickel, Request, Response};
use nickel::{Middleware, HttpRouter, StaticFilesHandler};
use nickel::{MiddlewareResult};
use plugin::Extensible;
use rustc_serialize::hex::FromHex;
use rustc_serialize::json;
use typemap::Key;

mod p150;

static MAX_CYCLES: i32 = 100000; // maximum number of cycles CPU can execute to prevent DoS attacks.

struct CpuDb;
impl Key for CpuDb { type Value = Arc<RwLock<CpuServ>>; }

struct CpuClient {
	pub state: P150Cpu,
	pub last:  P150Cpu,
}

/// A database of multiple uniquely identified P150 CPU emulators.
///
/// Each emulator maintains its own state and can be recalled
/// using the integer identification.
struct CpuServ {
	next_cpu: i32,
	database: HashMap<i32, CpuClient>,
}

impl CpuServ {
	fn new() -> CpuServ {
		CpuServ {
			next_cpu: 0,
			database: HashMap::new(),
		}
	}
}

/// Injects a CpuServ into the current request
struct CpuMw { db: Arc<RwLock<CpuServ>> }

impl Middleware for CpuMw {
	fn invoke<'a,'b>(&'a self, req: &mut Request<'b, 'a, 'b>, res: Response<'a>) -> MiddlewareResult<'a> {
		// inject db into response
		let map = req.extensions_mut();
		map.insert::<CpuDb>(self.db.clone());

		Ok(nickel::Continue(res))
	}
}

fn main() {
	let mut server = Nickel::new();
	let mut router = Nickel::router();
	
	let cpus = CpuServ::new();
	let cpu_mw = CpuMw { db: Arc::new(RwLock::new(cpus)) };

	router.post("/cpu/new", cpu_new);
	router.post("/cpu/:id/tick", cpu_tick);
	router.post("/cpu/:id/back", cpu_back);
	router.post("/cpu/:id/run", cpu_run);
	router.post("/cpu/:id/load", cpu_load);
	router.get("/cpu/:id/dump", cpu_dump);
	// TODO: /cpu/:id/tick, /cpu/:id/reset
	// TODO: /cpu/:id/save ???

	router.get("/", index_show);
	router.get("/about", about_show);

	server.utilize(StaticFilesHandler::new("./public"));
	server.utilize(cpu_mw);
	server.utilize(router);
	
	server.listen("0.0.0.0:3042");
}

fn about_show<'a>(_req: &mut Request, res: Response<'a>) -> MiddlewareResult<'a> {
	let mut data = HashMap::new();
	data.insert("title", "About P150");
	res.render("assets/views/about.html", &data)
}

fn index_show<'a>(_req: &mut Request, res: Response<'a>) -> MiddlewareResult<'a> {
	let mut data = HashMap::new();
	data.insert("title", "P150 Emulator");
	res.render("assets/views/index.html", &data)
}

/// POST /cpu/new: creates a new CPU and adds it to the CpuServ with the next
/// available ID number.
fn cpu_new<'a>(req: &mut Request, response: Response<'a>) -> MiddlewareResult<'a> {
	match req.extensions().get::<CpuDb>() {
		Some(cpuserv) => {
			let mut w_cpuserv = cpuserv.write().unwrap();
			let cpu_no = w_cpuserv.next_cpu;

			let client = CpuClient {
				state: P150Cpu::new(),
				last:  P150Cpu::new(),
			};

			w_cpuserv.database.insert(cpu_no, client);
			w_cpuserv.next_cpu += 1;

			response.send(json::encode(&cpu_no).ok().unwrap())
		},
		None => { response.send("did not find cpu serv") }
	}
}


/// POST /cpu/:id/load
/// Overwrites the CPUs system memory with response data.
/// The current machine state is then dumped into the response as JSON.
fn cpu_load<'a>(req: &mut Request, response: Response<'a>) -> MiddlewareResult<'a> {
	let id = req.param("id").parse::<i32>().ok().expect("invalid cpu id");
	let mem_js = req.json_as::<Vec<String>>().unwrap();
	let mem_native: Vec<u8> = mem_js.iter().map(|hex| {
		match hex[..].from_hex() {
			Ok(decoded) => { if decoded.len() > 0 { decoded[0] } else { 0xFF } },
			Err(msg) => { debug!("error reading json memory: {}", msg); 0xFF },
		}
	}).collect();

	info!("length of mem instructions: {}", mem_native.len());

	// pair odd u8s with even u8s to produce u16 ops.
	let even_mn = mem_native.iter().enumerate().filter(|&(idx, _)| { idx % 2 == 0 });
	let odd_mn  = mem_native.iter().enumerate().filter(|&(idx, _)| { idx % 2 != 0 });
	let ops: Vec<u16> = even_mn.zip(odd_mn).map(|((_, &msb), (_, &lsb))| {
		((msb as u16) << 8) | (lsb as u16)
	}).collect();

	match req.extensions().get::<CpuDb>() {
		Some(cpuserv) => {
			match cpuserv.write().unwrap().database.get_mut(&id) {
				Some(cpu) => {
					cpu.state.init_mem(&ops[..]); // TODO: load from req.
					cpu.last = cpu.state;

					response.send(format!("{}", cpu.state.js_dump()))
				},
				None => { response.send("did not find the cpu") },
			}
		},

		None => { response.send("did not find cpu serv") }
	}
}

/// POST /cpu/:id/dump
/// Dumps the CPUs current state to a JSON format.
/// This request does not modify the CPU state in any way.
fn cpu_dump<'a>(req: &mut Request, response: Response<'a>) -> MiddlewareResult<'a> {
	let id = req.param("id").parse::<i32>().ok().expect("unable to dump invalid cpu id");
	match req.extensions().get::<CpuDb>() {
		Some(cpuserv) => {
			let r_serv = cpuserv.read().unwrap();
			let cpu    = r_serv.database.get(&id).unwrap();

			response.send(format!("{}", cpu.state.js_dump()))
		},
		None => { response.send("did not find cpu serv") }
	}
}


/// POST /cpu/:id/tick
/// Resumes execution from the next instruction; running the machine to completion.
/// The resultant state is then dumped to the console.
fn cpu_tick<'a>(req: &mut Request, response: Response<'a>) -> MiddlewareResult<'a> {
	let id = req.param("id").parse::<i32>().ok().expect("unable to tick invalid cpu id");

	match req.extensions().get::<CpuDb>() {
		Some(cpuserv) => {
			let mut w_serv = cpuserv.write().unwrap();
			match w_serv.database.get_mut(&id) {
				Some(cpu) => {
					cpu.last = cpu.state;
					cpu.state.tick(); 
					response.send(format!("{}", cpu.state.js_dump())) 
				},

				None => { response.send("did not find cpu") }
			}
		},

		None => { response.send("did not find cpu serv") }
	}
}

/// POST /cpu/:id/back
/// Rewinds the CPU to the last good state.
/// States are snapshotted _before_ a /tick or /run
fn cpu_back<'a>(req: &mut Request, response: Response<'a>) -> MiddlewareResult<'a> {
	let id = req.param("id").parse::<i32>().ok().expect("unable to tick invalid cpu id");

	match req.extensions().get::<CpuDb>() {
		Some(cpuserv) => {
			let mut w_serv = cpuserv.write().unwrap();
			match w_serv.database.get_mut(&id) {
				Some(cpu) => { response.send(format!("{}", cpu.last.js_dump())) },
				None => { response.send("did not find the cpu") },
			}
		},

		None => { response.send("did not find cpu serv") },
	}
}


/// POST /cpu/:id/run
/// Resumes execution from the next instruction; running the machine to completion.
/// The resultant state is then dumped to the console.
fn cpu_run<'a>(req: &mut Request, response: Response<'a>) -> MiddlewareResult<'a> {
	let id = req.param("id").parse::<i32>().ok().expect("unable to run invalid cpu id");
	
	match req.extensions().get::<CpuDb>() {
		Some(cpuserv) => {
			let mut cycles = 0i32;
			let mut w_serv = cpuserv.write().unwrap();
			let cpu = w_serv.database.get_mut(&id).unwrap();
			cpu.last = cpu.state;

			loop {
				if cycles >= MAX_CYCLES { println!("CPU executed > {} cycles.", MAX_CYCLES); break; }
				cycles += 1;

				match cpu.state.tick() {
					CpuState::Halt => { break; },
					CpuState::Continue => { continue; },
				}
			}

			response.send(format!("{}", cpu.state.js_dump()))
		},
		None => { response.send("did not find cpu serv") }
	}
}
