#![recursion_limit = "128"]

pub fn main() -> oai::err::Result<()> {
    use futures::{Future, IntoFuture};

    // let client = oai::Client::new("https://demo.dspace.org/oai/")?;

    // let fut = {
    //     let client = std::sync::Arc::new(oai::Client::new("https://repository.tcu.edu/oai/")?);
    //     let params = oai::list_records::Params {
    //         set: Some("col_116099117_11012".into()),
    //         ..oai::list_records::Params::default()
    //     };

    //     client
    //         .list_records_all::<oai::pref::Xoai>(Some(params))
    //         .map_err(|e| {
    //             println!("{:?}", e);
    //             ()
    //         })
    //         .map(|t| {
    //             println!("{:?}", t.records.len());
    //             ()
    //         })
    //     // .map(|t| { println!("{}", &serde_json::to_string_pretty(&t).unwrap()); () } )
    //     // .and_then(move |t| {
    //     //     t.get_next(client_2.as_ref())
    //     //         .map(|t| { println!("{:?}", t); () } )
    //     //         .map_err(|e| { println!("{:?}", e); () } )
    //     // })
    // };

    let fut = {
        let client = std::sync::Arc::new(oai::Client::new("https://repository.tcu.edu/oai/")?);

        client
            .get_record::<oai::pref::OaiDc>("oai:repository.tcu.edu:116099117/23673")
            .map_err(|e| {
                println!("{:?}", e);
                ()
            })
            .map(|t| {
                println!("{:?}", t);
                ()
            })
        // .map(|t| { println!("{}", &serde_json::to_string_pretty(&t).unwrap()); () } )
        // .and_then(move |t| {
        //     t.get_next(client_2.as_ref())
        //         .map(|t| { println!("{:?}", t); () } )
        //         .map_err(|e| { println!("{:?}", e); () } )
        // })
    };


    // fut.wait();

    let _ = tokio::run(fut);

    Ok(())
}
