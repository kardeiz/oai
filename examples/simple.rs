pub fn main() -> oai::err::Result<()> {
    use futures::{Future, IntoFuture};

    let client = oai::Client::new("https://demo.dspace.org/oai/")?;

    let fut = client
        .list_records::<oai::pref::OaiDc>()
        .map_err(|e| println!("{:?}", e))
        .map(|pkg| println!("{:?}", pkg));

    let _ = tokio::run(fut);

    Ok(())
}
