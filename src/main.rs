#![feature(phase)]
#[phase(plugin, link)] extern crate log;
#[phase(plugin)] extern crate nickel_macros;
extern crate nickel;
extern crate serialize;

use p150::P150Cpu;

use std::collections::HashMap;
use std::io::net::ip::Ipv4Addr;
use std::sync::{Arc,RWLock};
use nickel::{Nickel, Request, Response};
use nickel::{Middleware, HttpRouter, StaticFilesHandler};
use nickel::{MiddlewareResult, NickelError};

mod p150;

struct CpuMw { db: Arc<RWLock<CpuServ>> }
struct CpuServ {
	next_cpu: i32,
	database: HashMap<i32, P150Cpu>,
}

impl Middleware for CpuMw {
	/// Injects the protected CPU map into the request
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

	fn cpu_new(req: &Request, response: &mut Response) {
		match req.map.find::<Arc<RWLock<CpuServ>>>() {
			Some(cpuserv) => {
				let mut w_cpuserv = cpuserv.write();
				let cpu_no = w_cpuserv.next_cpu;

				w_cpuserv.database.insert(cpu_no, P150Cpu::new());
				w_cpuserv.next_cpu += 1;

				response.send(format!("your cpu is # {}", w_cpuserv.next_cpu));
			},
			None => { response.send("did not find cpu serv"); }
		}
	}

	router.get("/", index_show);
	router.get("/cpu/new", cpu_new);

	server.utilize(CpuMw { db: Arc::new(RWLock::new(CpuServ::new())) });
	server.utilize(StaticFilesHandler::new("./public"));
	server.utilize(router);
	
	server.listen(Ipv4Addr(0,0,0,0), 3200);
}

