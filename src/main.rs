#![feature(phase)]
#[phase(plugin, link)] extern crate log;
#[phase(plugin)] extern crate nickel_macros;
extern crate nickel;
extern crate serialize;

use p150::P150Cpu;

use serialize::json;
use serialize::hex::FromHex;
use std::collections::HashMap;
use std::io::net::ip::Ipv4Addr;
use std::sync::{Arc,RWLock};
use nickel::{JsonBody, Nickel, Request, Response};
use nickel::{Middleware, HttpRouter, StaticFilesHandler};
use nickel::{MiddlewareResult, NickelError};

mod p150;

static MAX_CYCLES: i32 = 100000; // maximum number of cycles CPU can execute to prevent DoS attacks.

/// Injects a mutex-protected CpuServ into request map when invoked
struct CpuMw { db: Arc<RWLock<CpuServ>> }

/// A database of multiple uniquely identified P150 CPU emulators.
///
/// Each emulator maintains its own state and can be recalled
/// using the integer identification.
struct CpuServ {
	next_cpu: i32,
	database: HashMap<i32, P150Cpu>,
}

impl Middleware for CpuMw {
	fn invoke(&self, req: &mut Request, _res: &mut Response) -> MiddlewareResult {
		req.map.insert(self.db.clone());
		Ok(nickel::Continue)
	}
}

impl CpuServ {
	fn new() -> CpuServ {
		CpuServ {
			next_cpu: 0,
			database: HashMap::new(),
		}
	}
}

fn main() {
	let mut server = Nickel::new();
	let mut router = Nickel::router();
	
	fn index_show(_request: &Request, response: &mut Response) {
		let mut page = HashMap::new();
		page.insert("title", "P150 Emulator");

		response.render("./assets/views/index.html", &page);
	}

	fn about_show(_request: &Request, response: &mut Response) {
		let mut page = HashMap::new();
		page.insert("title", "P150 Emulator");

		response.render("./assets/views/about.html", &page);
	}

	/// POST /cpu/new: creates a new CPU and adds it to the CpuServ with the next
	/// available ID number.
	fn cpu_new(req: &Request, response: &mut Response) {
		match req.map.find::<Arc<RWLock<CpuServ>>>() {
			Some(cpuserv) => {
				let mut w_cpuserv = cpuserv.write();
				let cpu_no = w_cpuserv.next_cpu;

				w_cpuserv.database.insert(cpu_no, P150Cpu::new());
				w_cpuserv.next_cpu += 1;

				response.send(json::encode(&cpu_no));
			},
			None => { response.send("did not find cpu serv"); }
		}
	}

	/// POST /cpu/:id/load
	/// Overwrites the CPUs system memory with response data.
	/// The current machine state is then dumped into the response as JSON.
	fn cpu_load(req: &Request, response: &mut Response) {
		let id = from_str::<i32>(req.param("id")).unwrap();
		let mem_js = req.json_as::<Vec<String>>().unwrap();
		let mem_native: Vec<u8> = mem_js.iter().map(|hex| {
			match hex.as_slice().from_hex() {
				Ok(decoded) => { if decoded.len() > 0 { decoded[0] } else { 0xFF } },
				Err(msg) => { debug!("error reading json memory: {}", msg); 0xFF },
			}
		}).collect();

		info!("length of mem instructions: {}", mem_native.len());

		// pair odd u8s with even u8s to produce u16 ops.
		let even_mn = mem_native.iter().enumerate().filter(|&(idx, _)| { idx % 2 == 0 });
		let odd_mn  = mem_native.iter().enumerate().filter(|&(idx, _)| { idx % 2 != 0 });
		let ops: Vec<u16> = even_mn.zip(odd_mn).map(|((_, &msb), (_, &lsb))| {
			(msb as u16 << 8) | (lsb as u16)
		}).collect();

		match req.map.find::<Arc<RWLock<CpuServ>>>() {
			Some(cpuserv) => {
				match cpuserv.write().database.find_mut(&id) {
					Some(cpu) => {
						cpu.init_mem(ops.as_slice()); // TODO: load from req.
						response.send(format!("{}", cpu.js_dump()));
					},
					None => { response.send("did not find the cpu") },
				};
			},
			None => { response.send("did not find cpu serv"); }
		}
	}

	/// POST /cpu/:id/dump
	/// Dumps the CPUs current state to a JSON format.
	/// This request does not modify the CPU state in any way.
	fn cpu_dump(req: &Request, response: &mut Response) {
		let id = from_str::<i32>(req.param("id")).unwrap();
		match req.map.find::<Arc<RWLock<CpuServ>>>() {
			Some(cpuserv) => {
				let r_serv = cpuserv.read();
				let cpu        = r_serv.database.find(&id).unwrap();

				response.send(format!("{}", cpu.js_dump()));
			},
			None => { response.send("did not find cpu serv"); }
		}
	}


	/// POST /cpu/:id/tick
	/// Resumes execution from the next instruction; running the machine to completion.
	/// The resultant state is then dumped to the console.
	fn cpu_tick(req: &Request, response: &mut Response) {
		let id = from_str::<i32>(req.param("id")).unwrap();
		match req.map.find::<Arc<RWLock<CpuServ>>>() {
			Some(cpuserv) => {
				let mut w_serv = cpuserv.write();
				let cpu        = w_serv.database.find_mut(&id).unwrap();
				let _          = cpu.tick();

				response.send(format!("{}", cpu.js_dump()));
			},
			None => { response.send("did not find cpu serv"); }
		}
	}


	/// POST /cpu/:id/run
	/// Resumes execution from the next instruction; running the machine to completion.
	/// The resultant state is then dumped to the console.
	fn cpu_run(req: &Request, response: &mut Response) {
		let id = from_str::<i32>(req.param("id")).unwrap();
		
		match req.map.find::<Arc<RWLock<CpuServ>>>() {
			Some(cpuserv) => {
				let mut cycles = 0i32;
				let mut w_serv = cpuserv.write();
				let cpu    = w_serv.database.find_mut(&id).unwrap();
				loop {
					if cycles >= MAX_CYCLES { println!("CPU executed > {} cycles.", MAX_CYCLES); break; }
					cycles += 1;

					match cpu.tick() {
						p150::Halt => { break; },
						p150::Continue => { continue; },
					}
				}

				response.send(format!("{}", cpu.js_dump()));
			},
			None => { response.send("did not find cpu serv"); }
		}
	}

	router.get("/", index_show);
	router.get("/about", about_show);

	router.post("/cpu/new", cpu_new);
	router.post("/cpu/:id/tick", cpu_tick);
	router.post("/cpu/:id/run", cpu_run);
	router.post("/cpu/:id/load", cpu_load);
	router.get("/cpu/:id/dump", cpu_dump);
	// TODO: /cpu/:id/tick, /cpu/:id/reset
	// TODO: /cpu/:id/save ???

	server.utilize(CpuMw { db: Arc::new(RWLock::new(CpuServ::new())) });
	server.utilize(StaticFilesHandler::new("./public"));
	server.utilize(Nickel::json_body_parser());
	server.utilize(router);
	
	server.listen(Ipv4Addr(0,0,0,0), 3200);
}

