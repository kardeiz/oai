// #![deny(warnings)]

// extern crate futures;
// extern crate reqwest;
// extern crate tokio;

// use futures::{Future, Stream};
// use reqwest::r#async::{Client, Decoder};
// use std::{
//     io::{self, Cursor},
//     mem
// };

// fn fetch() -> impl Future<Item = (), Error = ()> {
//     Client::new()
//         .get("https://repository.tcu.edu/oai/request?verb=ListRecords&metadataPrefix=oai_dc")
//         .send()
//         .and_then(|mut res| {
//             println!("{}", res.status());

//             let body = mem::replace(res.body_mut(), Decoder::empty());
//             body.concat2()
//         })
//         .map_err(|err| println!("request error: {}", err))
//         .map(|body| {
//             let mut body = Cursor::new(body);
//             let _ = io::copy(&mut body, &mut io::stdout()).map_err(|err| {
//                 println!("stdout error: {}", err);
//             });
//         })
// }

// fn main() { tokio::run(fetch()); }
