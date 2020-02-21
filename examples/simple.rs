#![recursion_limit = "128"]

#[tokio::main]
async fn main() -> oai::err::Result<()> {

    let client = oai::Client::new("https://repository.tcu.edu/oai/request")?;

    // let res = client.get_record::<oai::metadata::OaiDc>("oai:repository.tcu.edu:116099117/23673").await?;

    let params = oai::list_records::Params {
        set: Some("col_116099117_11012".into()),
        ..oai::list_records::Params::default()
    };

    let res = client.list_records_all::<oai::metadata::Xoai>(Some(params)).await?;

    println!("{}", serde_json::to_string_pretty(&res).unwrap());

    Ok(())

    // use futures::{Future, IntoFuture};

    // // let client = oai::Client::new("https://demo.dspace.org/oai/")?;

    // // let fut = {
    // //     let client = std::sync::Arc::new(oai::Client::new("https://repository.tcu.edu/oai/")?);
    // //     let params = oai::list_records::Params {
    // //         set: Some("col_116099117_11012".into()),
    // //         ..oai::list_records::Params::default()
    // //     };

    // //     client
    // //         .list_records_all::<oai::pref::Xoai>(Some(params))
    // //         .map_err(|e| {
    // //             println!("{:?}", e);
    // //             ()
    // //         })
    // //         .map(|t| {
    // //             println!("{:?}", t.records.len());
    // //             ()
    // //         })
    // //     // .map(|t| { println!("{}", &serde_json::to_string_pretty(&t).unwrap()); () } )
    // //     // .and_then(move |t| {
    // //     //     t.get_next(client_2.as_ref())
    // //     //         .map(|t| { println!("{:?}", t); () } )
    // //     //         .map_err(|e| { println!("{:?}", e); () } )
    // //     // })
    // // };

    // let fut = {
    //     let client = std::sync::Arc::new(oai::Client::new("https://repository.tcu.edu/oai/")?);

    //     client
    //         .get_record::<oai::pref::OaiDc>("oai:repository.tcu.edu:116099117/23673")
    //         .map_err(|e| {
    //             println!("{:?}", e);
    //             ()
    //         })
    //         .map(|t| {
    //             println!("{:?}", t);
    //             ()
    //         })
    //     // .map(|t| { println!("{}", &serde_json::to_string_pretty(&t).unwrap()); () } )
    //     // .and_then(move |t| {
    //     //     t.get_next(client_2.as_ref())
    //     //         .map(|t| { println!("{:?}", t); () } )
    //     //         .map_err(|e| { println!("{:?}", e); () } )
    //     // })
    // };


    // // fut.wait();

    // let _ = tokio::run(fut);

    // Ok(())
}
