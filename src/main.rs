#![feature(phase)]
#[phase(plugin, link)] extern crate log;
#[phase(plugin)] extern crate nickel_macros;
extern crate nickel;
extern crate serialize;

use std::collections::HashMap;
use std::io::net::ip::Ipv4Addr;
use nickel::{Nickel, Request, Response};
use nickel::{HttpRouter, StaticFilesHandler};

fn main() {
	let mut server = Nickel::new();
	let mut router = Nickel::router();
	
	fn index_show(_request: &Request, response: &mut Response) {
		let mut page = HashMap::new();
		page.insert("title", "P150 Emulator");

		response.render("./assets/views/index.html", &page);
	}

	router.get("/", index_show);
	server.utilize(router);
	server.utilize(StaticFilesHandler::new("./public"));
	
	server.listen(Ipv4Addr(0,0,0,0), 3200);
}

