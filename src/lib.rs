pub mod err {
    pub use failure::{err_msg as msg, Error};
    pub type Result<T> = std::result::Result<T, Error>;
    // pub type BoxedFuture<T> = Box<futures::Future<Item = T, Error = Error>>;
}

pub extern crate roxmltree;

// use futures::{future::Either as EitherFuture, Future, IntoFuture, Stream};
// use serde_derive::{Deserialize, Serialize};
// use std::sync::Arc;

#[derive(Clone)]
pub struct Client {
    pub(crate) inner: reqwest::r#async::Client,
    pub(crate) base_url: url::Url
}

impl Client {
    pub fn new(url: &str) -> err::Result<Self> {
        Ok(Client { inner: reqwest::r#async::Client::new(), base_url: url.parse()? })
    }
}

pub mod pref {

    use serde_json::json;

    use serde_derive::{Deserialize, Serialize};

    use crate::err;
    use std::collections::HashMap;

    pub trait MetadataPrefix: std::fmt::Debug {
        type MetadataType: std::fmt::Debug;

        const AS_STR: &'static str;

        fn parse_metadata(node: &roxmltree::Node) -> err::Result<Self::MetadataType>;
    }

    #[derive(Debug)]
    pub enum OaiDc {}

    impl MetadataPrefix for OaiDc {
        type MetadataType = HashMap<String, Vec<Option<String>>>;

        const AS_STR: &'static str = "oai_dc";

        fn parse_metadata(node: &roxmltree::Node) -> err::Result<Self::MetadataType> {
            let mut map = HashMap::new();

            for dc_node in node
                .children()
                .filter(|x| x.has_tag_name("dc"))
                .flat_map(|x| x.children())
                .filter(|x| x.is_element())
            {
                map.entry(String::from(dc_node.tag_name().name()))
                    .or_insert_with(Vec::new)
                    .push(dc_node.text().map(String::from));
            }

            Ok(map)
        }
    }

    #[derive(Debug)]
    pub enum Xoai {}

    #[derive(Debug, Serialize)]
    pub struct XoaiElements(pub Vec<XoaiElement>);

    #[derive(Debug, Serialize)]
    pub struct XoaiElement {
        name: String,
        fields: Option<Vec<(Option<String>, Option<String>)>>,
        children: Option<Vec<Box<XoaiElement>>>
    }

    impl XoaiElement {
        fn from_node(node: &roxmltree::Node) -> err::Result<Self> {
            let name = node.attribute("name").ok_or_else(|| err::msg("No name"))?.to_string();
            let fields = node
                .children()
                .filter(|x| x.has_tag_name("field"))
                .map(|x| (x.attribute("name").map(String::from), x.text().map(String::from)))
                .collect::<Vec<_>>();
            let children = node
                .children()
                .filter(|x| x.has_tag_name("element"))
                .map(|x| XoaiElement::from_node(&x).map(Box::new))
                .collect::<err::Result<Vec<_>>>()?;
            Ok(XoaiElement {
                name,
                fields: if fields.is_empty() { None } else { Some(fields) },
                children: if children.is_empty() { None } else { Some(children) }
            })
        }
    }

    impl MetadataPrefix for Xoai {
        type MetadataType = XoaiElements;

        const AS_STR: &'static str = "xoai";

        fn parse_metadata(node: &roxmltree::Node) -> err::Result<Self::MetadataType> {
            let vec = node
                .children()
                .filter(|x| x.has_tag_name("metadata"))
                .flat_map(|x| x.children())
                .filter(|x| x.has_tag_name("element"))
                .map(|x| XoaiElement::from_node(&x))
                .collect::<err::Result<Vec<_>>>()?;

            Ok(XoaiElements(vec))
        }
    }

}

pub mod record {

    use chrono::prelude::*;
    use serde_derive::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize)]
    pub struct Header {
        pub identifier: String,
        pub datestamp: DateTime<Utc>,
        pub set_spec: Vec<String>
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct Record<T> {
        pub header: Header,
        pub metadata: T
    }
}

pub mod get_record {
    use futures::{future::Either as EitherFuture, Future, IntoFuture, Stream};
    use serde_derive::{Deserialize, Serialize};

    use chrono::prelude::*;
    use std::{marker::PhantomData, sync::Arc};

    use crate::{
        err,
        ext::NodeExt,
        pref::MetadataPrefix,
        record::{Header, Record},
        Client
    };

    #[derive(Debug, Serialize)]
    pub struct GetRecord<T, U> {
        pub response_date: DateTime<Utc>,
        pub record: Record<U>,
        #[serde(skip_serializing)]
        marker: PhantomData<T>
    }

    impl<T: MetadataPrefix> GetRecord<T, T::MetadataType> {

        fn build(text: String) -> err::Result<Self> {
            let doc = roxmltree::Document::parse(&text)?;

            let root = doc.root_element();

            let response_date = root.find_first_child_parsed_text("responseDate")?;

            let get_record = root.find_first_child("GetRecord")?;


            let record = get_record
                .children()
                .filter(|x| x.has_tag_name("record"))
                .flat_map(|x| -> err::Result<Record<T::MetadataType>> {
                    let header = x.find_first_child("header")?;

                    let header = Header {
                        identifier: header.find_first_child_parsed_text("identifier")?,
                        datestamp: header.find_first_child_parsed_text("datestamp")?,
                        set_spec: header
                            .children()
                            .filter(|y| y.has_tag_name("setSpec"))
                            .flat_map(|x| x.text().map(String::from))
                            .collect()
                    };

                    let metadata_node = x.find_first_child("metadata")?;

                    let metadata = T::parse_metadata(&metadata_node)?;

                    Ok(Record { header, metadata })
                })
                .next()
                .ok_or_else(|| err::msg("No such object"))?;

            Ok(GetRecord { response_date, record, marker: PhantomData })
        }
    }

    impl Client {
        pub fn get_record<'a, T: MetadataPrefix>(
            &self,
            identifier: &'a str
        ) -> impl Future<Item = GetRecord<T, T::MetadataType>, Error = err::Error>
        {
 
            #[derive(Serialize)]
            struct VerbedParams<'b> {
                identifier: &'b str,
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

            let params = VerbedParams { identifier, metadata_prefix: T::AS_STR, verb: "GetRecord" };

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
                .and_then(|text| {
                    GetRecord::<T, T::MetadataType>::build(text).into_future().from_err()
                });

            EitherFuture::A(rt)
        }
    }
}

pub mod list_records {

    use futures::{future::Either as EitherFuture, Future, IntoFuture, Stream};
    use serde_derive::{Deserialize, Serialize};

    use chrono::prelude::*;
    use std::{marker::PhantomData, sync::Arc};

    use crate::{
        err,
        ext::NodeExt,
        pref::MetadataPrefix,
        record::{Header, Record},
        Client
    };

    #[derive(Debug, Serialize)]
    pub struct ResumptionToken {
        value: Option<String>,
        complete_list_size: u64,
        cursor: u64
    }

    #[derive(Debug, Serialize)]
    pub struct ListRecords<T, U> {
        pub response_date: DateTime<Utc>,
        pub records: Vec<Record<U>>,
        pub resumption_token: Option<ResumptionToken>,
        #[serde(skip_serializing)]
        marker: PhantomData<T>
    }

    #[derive(Default, Debug, Serialize)]
    pub struct Params {
        pub from: Option<DateTime<Utc>>,
        pub until: Option<DateTime<Utc>>,
        pub set: Option<String>
    }

    impl<T: MetadataPrefix> ListRecords<T, T::MetadataType> {
        pub fn has_next(&self) -> bool {
            self.resumption_token.as_ref().and_then(|x| x.value.as_ref()).is_some()
        }

        pub fn get_next(
            self,
            client: &Client
        ) -> impl Future<Item = ListRecords<T, T::MetadataType>, Error = err::Error>
        {
            if let Some(rt) = self
                .resumption_token
                .as_ref()
                .and_then(|x| x.value.as_ref())
                .map(|x| String::from(x as &str))
            {
                EitherFuture::A(client.list_records_from_resumption_token(rt))
            } else {
                EitherFuture::B(futures::future::err(err::msg("No more results")))
            }
        }

        fn build(text: String) -> err::Result<ListRecords<T, T::MetadataType>> {
            let doc = roxmltree::Document::parse(&text)?;

            let root = doc.root_element();

            let response_date = root.find_first_child_parsed_text("responseDate")?;

            let list_records = root.find_first_child("ListRecords")?;

            let resumption_token =
                list_records.find_first_child("resumptionToken").ok().and_then(|x| {
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
                .flat_map(|x| -> err::Result<Record<T::MetadataType>> {
                    let header = x.find_first_child("header")?;

                    let header = Header {
                        identifier: header.find_first_child_parsed_text("identifier")?,
                        datestamp: header.find_first_child_parsed_text("datestamp")?,
                        set_spec: header
                            .children()
                            .filter(|y| y.has_tag_name("setSpec"))
                            .flat_map(|x| x.text().map(String::from))
                            .collect()
                    };

                    let metadata_node = x.find_first_child("metadata")?;

                    let metadata = T::parse_metadata(&metadata_node)?;

                    Ok(Record { header, metadata })
                })
                .collect();

            Ok(ListRecords { response_date, records, resumption_token, marker: PhantomData })
        }
    }

    impl Client {
        pub fn list_records<T: MetadataPrefix>(
            &self,
            params: Option<Params>
        ) -> impl Future<Item = ListRecords<T, T::MetadataType>, Error = err::Error>
        {
            let params = params.unwrap_or_else(Params::default);

            #[derive(Serialize)]
            struct VerbedParams {
                #[serde(flatten)]
                params: Params,
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

            let params = VerbedParams { params, metadata_prefix: T::AS_STR, verb: "ListRecords" };

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
                .and_then(|text| {
                    ListRecords::<T, T::MetadataType>::build(text).into_future().from_err()
                });

            EitherFuture::A(rt)
        }

        pub fn list_records_from_resumption_token<T: MetadataPrefix>(
            &self,
            resumption_token: String
        ) -> impl Future<Item = ListRecords<T, T::MetadataType>, Error = err::Error>
        {
            #[derive(Serialize)]
            struct VerbedParams {
                #[serde(rename = "resumptionToken")]
                resumption_token: String,
                verb: &'static str
            }

            let mut url = self.base_url.clone();

            if let Ok(mut path_segments) = url.path_segments_mut() {
                path_segments.pop_if_empty().push("request");
            } else {
                return EitherFuture::B(futures::future::err(err::msg("Cannot set path")));
            }

            let params = VerbedParams { resumption_token, verb: "ListRecords" };

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
                .and_then(|text| {
                    ListRecords::<T, T::MetadataType>::build(text).into_future().from_err()
                });

            EitherFuture::A(rt)
        }

        pub fn list_records_all<T: MetadataPrefix>(
            &self,
            params: Option<Params>
        ) -> impl Future<Item = ListRecords<T, T::MetadataType>, Error = err::Error>
        {
            fn run<T: MetadataPrefix>(
                client: Client,
                params: Option<Params>
            ) -> impl Future<Item = ListRecords<T, T::MetadataType>, Error = err::Error>
            {
                use futures::future::{loop_fn, Loop};

                loop_fn(EitherFuture::A(client.list_records(params)), move |list_records_fut| {
                    let client = client.clone();
                    list_records_fut.and_then(move |mut list_records| {
                        if list_records.has_next() {
                            let mut prev_records =
                                list_records.records.drain(..).collect::<Vec<_>>();
                            let rt = list_records.get_next(&client).map(|mut x| {
                                prev_records.extend(x.records.drain(..));
                                x.records = prev_records;
                                x
                            });
                            Ok(Loop::Continue(EitherFuture::B(rt)))
                        } else {
                            Ok(Loop::Break(list_records))
                        }
                    })
                })
            }

            run(self.clone(), params)
        }
    }
}

mod ext {

    use roxmltree::{ExpandedName, Node};

    pub(crate) trait NodeExt<'a, 'd>: Sized {
        fn find_first_child<I: Into<ExpandedName<'a>>>(
            &self,
            tag_name: I
        ) -> crate::err::Result<Node<'a, 'd>>;
        fn find_first_child_parsed_text<P: std::str::FromStr, I: Into<ExpandedName<'a>>>(
            &'a self,
            tag_name: I
        ) -> crate::err::Result<P>;
    }

    impl<'a, 'd: 'a> NodeExt<'a, 'd> for Node<'a, 'd> {
        fn find_first_child<I: Into<ExpandedName<'a>>>(
            &self,
            tag_name: I
        ) -> crate::err::Result<Node<'a, 'd>>
        {
            let tag_name = tag_name.into();
            self.children()
                .filter(|x| x.has_tag_name(tag_name))
                .next()
                .ok_or_else(|| crate::err::msg(format!("No such tag: {}", tag_name.name())))
        }

        fn find_first_child_parsed_text<P: std::str::FromStr, I: Into<ExpandedName<'a>>>(
            &'a self,
            tag_name: I
        ) -> crate::err::Result<P>
        {
            let tag_name = tag_name.into();
            self.find_first_child(tag_name)?
                .text()
                .and_then(|x| x.parse().ok())
                .ok_or_else(|| crate::err::msg(format!("No text for: {}", tag_name.name())))
        }
    }

}
