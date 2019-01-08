pub mod err {
    pub use failure::{err_msg as msg, Error};
    pub type Result<T> = std::result::Result<T, Error>;
    pub type BoxedFuture<T> = Box<futures::Future<Item = T, Error = Error>>;
}

use futures::{future::Either as EitherFuture, Future, IntoFuture, Stream};
use serde_derive::{Deserialize, Serialize};

pub struct Client {
    inner: reqwest::r#async::Client,
    base_url: url::Url
}

impl Client {
    pub fn new(url: &str) -> err::Result<Self> {
        Ok(Client { inner: reqwest::r#async::Client::new(), base_url: url.parse()? })
    }
}

pub mod pref {

    use serde_json::json;

    pub trait MetadataPrefix: std::fmt::Debug {
        type MetadataType: std::fmt::Debug;

        fn as_str() -> &'static str;
        fn parse_metadata(node: &roxmltree::Node) -> crate::err::Result<Self::MetadataType>;
    }

    #[derive(Debug)]
    pub struct OaiDc;
    #[derive(Debug)]
    pub struct Xoai;

    impl MetadataPrefix for OaiDc {
        type MetadataType = serde_json::Value;

        fn as_str() -> &'static str { "oai_dc" }

        fn parse_metadata(node: &roxmltree::Node) -> crate::err::Result<Self::MetadataType> {
            let mut map = std::collections::HashMap::new();

            for dc_node in node
                .children()
                .filter(|x| x.has_tag_name("dc"))
                .flat_map(|x| x.children())
                .filter(|x| x.is_element())
            {
                map.entry(String::from(dc_node.tag_name().name()))
                    .or_insert_with(Vec::new)
                    .push(json!(dc_node.text()));
            }

            Ok(json!(map))
        }
    }

    impl MetadataPrefix for Xoai {
        type MetadataType = serde_json::Value;

        fn as_str() -> &'static str { "xoai" }

        fn parse_metadata(_node: &roxmltree::Node) -> crate::err::Result<Self::MetadataType> {
            unimplemented!()
        }
    }

}

pub mod record {

    use chrono::prelude::*;

    #[derive(Debug, Clone)]
    pub struct Header {
        pub identifier: String,
        pub datestamp: DateTime<Utc>,
        pub set_spec: Vec<String>
    }

    #[derive(Debug, Clone)]
    pub struct Record<T> {
        pub header: Header,
        pub metadata: T
    }
}

pub mod list_records {

    use futures::{future::Either as EitherFuture, Future, IntoFuture, Stream};
    use serde_derive::{Deserialize, Serialize};

    use chrono::prelude::*;
    use std::marker::PhantomData;

    use crate::{err, ext::NodeExt, pref, record, Client};

    impl<T: pref::MetadataPrefix> ListRecords<T> {
        fn build(text: String) -> err::Result<ListRecords<T>> {
            let doc = roxmltree::Document::parse(&text)?;

            // for child in doc.root().children() {
            //     println!("{:?}", child.tag_name());
            // }

            let root = doc.root_element();

            let response_date = root
                .children()
                .filter(|x| x.has_tag_name("responseDate"))
                .next()
                .and_then(|x| x.text().and_then(|x| x.parse().ok()))
                .ok_or_else(|| err::msg("No response date"))?;

            let list_records = root
                .children()
                .filter(|x| x.has_tag_name("ListRecords"))
                .next()
                .ok_or_else(|| err::msg("No ListRecords node"))?;

            let resumption_token = list_records
                .children()
                .filter(|x| x.has_tag_name("resumptionToken"))
                .next()
                .and_then(|x| {
                    match (
                        x.text(),
                        x.attribute("completeListSize").and_then(|x| x.parse().ok()),
                        x.attribute("cursor").and_then(|x| x.parse().ok())
                    ) {
                        (value, Some(complete_list_size), Some(cursor)) => Some(ResumptionToken {
                            value: value.map(String::from),
                            complete_list_size,
                            cursor
                        }),
                        _ => None
                    }
                });

            let records = list_records
                .children()
                .filter(|x| x.has_tag_name("record"))
                .flat_map(|x| -> err::Result<record::Record<T::MetadataType>> {
                    let header = x.first("header")?;

                    let header = record::Header {
                        identifier: header
                            .first("identifier")?
                            .text()
                            .ok_or_else(|| err::msg("No identifier"))?
                            .into(),
                        datestamp: header
                            .first("datestamp")?
                            .text()
                            .ok_or_else(|| err::msg("No datestamp"))?
                            .parse()?,
                        set_spec: header
                            .children()
                            .filter(|y| y.has_tag_name("setSpec"))
                            .flat_map(|x| x.text().map(String::from))
                            .collect()
                    };

                    let metadata_node = x.first("metadata")?;

                    let metadata = T::parse_metadata(&metadata_node)?;

                    Ok(record::Record { header, metadata })
                })
                .collect();

            Ok(ListRecords { response_date, records, resumption_token, marker: PhantomData })
        }
    }

    #[derive(Debug)]
    pub struct ResumptionToken {
        value: Option<String>,
        complete_list_size: u64,
        cursor: u64
    }

    #[derive(Debug)]
    pub struct ListRecords<T: pref::MetadataPrefix> {
        response_date: DateTime<Utc>,
        records: Vec<record::Record<T::MetadataType>>,
        resumption_token: Option<ResumptionToken>,
        marker: PhantomData<T>
    }

    impl Client {
        pub fn list_records<T: pref::MetadataPrefix>(
            &self
        ) -> impl Future<Item = ListRecords<T>, Error = err::Error> {
            #[derive(Serialize)]
            struct VerbedParams {
                #[serde(rename = "metadataPrefix")]
                metadata_prefix: &'static str,
                verb: &'static str
            }

            let mut url = self.base_url.clone();

            if let Ok(mut path_segments) = url.path_segments_mut() {
                path_segments.pop_if_empty().push("request");
            } else {
                return EitherFuture::B(futures::future::err(err::msg("Cannot set path")));
            }

            let params = VerbedParams { metadata_prefix: T::as_str(), verb: "ListRecords" };

            if let Ok(query) = serde_urlencoded::to_string(&params) {
                url.set_query(Some(&query));
            } else {
                return EitherFuture::B(futures::future::err(err::msg("Cannot set query")));
            }

            let rt = self
                .inner
                .get(url)
                .send()
                .and_then(|mut res| {
                    let body =
                        std::mem::replace(res.body_mut(), reqwest::r#async::Decoder::empty());
                    body.concat2()
                })
                .from_err()
                .and_then(|body| String::from_utf8(body.to_vec()).into_future().from_err())
                .and_then(|text| ListRecords::<T>::build(text).into_future().from_err());

            EitherFuture::A(rt)
        }
    }
}

mod ext {

    use roxmltree::{ExpandedName, Node};

    pub(crate) trait NodeExt<'a, 'd>: Sized {
        fn first<I: Into<ExpandedName<'a>>>(&self, tag_name: I)
            -> crate::err::Result<Node<'a, 'd>>;
    }

    impl<'a, 'd: 'a> NodeExt<'a, 'd> for Node<'a, 'd> {
        fn first<I: Into<ExpandedName<'a>>>(
            &self,
            tag_name: I
        ) -> crate::err::Result<Node<'a, 'd>>
        {
            let tag_name = tag_name.into();
            self.children()
                .filter(|x| x.has_tag_name(tag_name))
                .next()
                .ok_or_else(|| crate::err::msg(format!("No tag named: {:?}", tag_name)))
        }
    }

}
