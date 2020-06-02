/// Error handling
pub mod err {

    use std::fmt::Display;

    /// The error enum
    #[derive(Debug)]
    pub enum Error {
        InvalidArgument(String),
        InvalidResponse(String),
        Internal(String),
        NotFound(String),
    }

    impl Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            use Error::*;
            match self {
                InvalidArgument(ref s) => write!(f, "Invalid argument: '{}'", s),
                InvalidResponse(ref s) => write!(f, "Invalid response: '{}'", s),
                Internal(ref s) => write!(f, "Internal error: '{}'", s),
                NotFound(ref s) => write!(f, "Not found: '{}'", s),
            }
        }
    }

    impl std::error::Error for Error {}

    pub fn invalid_argument<T: Display>(t: T) -> Error {
        Error::InvalidArgument(t.to_string())
    }

    pub fn invalid_response<T: Display>(t: T) -> Error {
        Error::InvalidArgument(t.to_string())
    }

    pub fn internal<T: Display>(t: T) -> Error {
        Error::Internal(t.to_string())
    }

    pub fn not_found<T: Display>(t: T) -> Error {
        Error::NotFound(t.to_string())
    }


    /// Result wrapper: `Result<T, Error>`
    pub type Result<T> = std::result::Result<T, Error>;

}

use std::sync::Arc;

#[derive(Clone)]
pub struct Client {
    pub inner: Arc<reqwest::Client>,
    pub base_url: url::Url
}

impl Client {
    pub fn new<T: std::borrow::Borrow<str>>(url: T) -> err::Result<Self> {
        Ok(Client { 
            inner: Arc::new(reqwest::Client::new()), 
            base_url: url.borrow().parse().map_err(err::invalid_argument)?
        })
    }
}

pub mod metadata {

    use crate::err;
    use std::collections::HashMap;

    pub trait Format {
        type MetadataKind: std::fmt::Debug;

        const AS_STR: &'static str;

        fn parse_metadata(node: &roxmltree::Node) -> err::Result<Self::MetadataKind>;
    }

    pub enum OaiDc {}

    impl Format for OaiDc {
        type MetadataKind = HashMap<String, Vec<Option<String>>>;

        const AS_STR: &'static str = "oai_dc";

        fn parse_metadata(node: &roxmltree::Node) -> err::Result<Self::MetadataKind> {
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

    pub enum Xoai {}

    #[derive(Debug, serde::Serialize)]
    pub struct XoaiElements(pub Vec<XoaiElement>);

    #[derive(Debug, serde::Serialize)]
    pub struct XoaiElement {
        name: String,
        fields: Option<Vec<(Option<String>, Option<String>)>>,
        children: Option<Vec<Box<XoaiElement>>>
    }

    impl XoaiElement {
        fn from_node(node: &roxmltree::Node) -> err::Result<Self> {
            let name = node.attribute("name").ok_or_else(|| err::internal("No name"))?.to_string();
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

    impl Format for Xoai {
        type MetadataKind = XoaiElements;

        const AS_STR: &'static str = "xoai";

        fn parse_metadata(node: &roxmltree::Node) -> err::Result<Self::MetadataKind> {
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

    #[derive(Debug, Clone, serde::Serialize)]
    pub struct Header {
        pub identifier: String,
        pub datestamp: chrono::DateTime<chrono::Utc>,
        pub set_spec: Vec<String>
    }

    #[derive(Debug, Clone, serde::Serialize)]
    pub struct Record<T> {
        pub header: Header,
        pub metadata: T
    }

}

pub mod get_record {

    use chrono::prelude::*;
    use std::marker::PhantomData;

    use crate::{
        err,
        ext::NodeExt,
        metadata,
        Client
    };

    #[derive(Debug, serde::Serialize)]
    pub struct GetRecord<T, U> {
        pub response_date: DateTime<Utc>,
        pub record: metadata::Record<U>,
        #[serde(skip_serializing)]
        marker: PhantomData<T>
    }

    impl<T: metadata::Format> GetRecord<T, T::MetadataKind> {

        fn build(identifier: &str, text: String,) -> err::Result<Self> {
            let doc = roxmltree::Document::parse(&text).map_err(err::invalid_response)?;

            let root = doc.root_element();

            let response_date = root.find_first_child_parsed_text("responseDate")?;

            let get_record = root.find_first_child("GetRecord")?;

            let record = get_record
                .children()
                .filter(|x| x.has_tag_name("record"))
                .flat_map(|x| -> err::Result<metadata::Record<T::MetadataKind>> {
                    let header = x.find_first_child("header")?;

                    let header = metadata::Header {
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

                    Ok(metadata::Record { header, metadata })
                })
                .next()
                .ok_or_else(|| err::not_found(identifier))?;

            Ok(GetRecord { response_date, record, marker: PhantomData })
        }
    }

    impl Client {
        pub async fn get_record<T: metadata::Format>(
            &self,
            identifier: &str
        ) -> err::Result<GetRecord<T, T::MetadataKind>>
        {
 
            #[derive(serde::Serialize)]
            struct VerbedParams<'b> {
                identifier: &'b str,
                #[serde(rename = "metadataPrefix")]
                metadata_prefix: &'static str,
                verb: &'static str
            }

            let mut url = self.base_url.clone();

            let params = VerbedParams { identifier, metadata_prefix: T::AS_STR, verb: "GetRecord" };

            url.set_query(Some(&serde_urlencoded::to_string(&params).map_err(err::internal)?));

            let body = self.inner.get(url).send().await.map_err(err::internal)?
                .text().await.map_err(err::internal)?;

            Ok(GetRecord::<T, T::MetadataKind>::build(identifier, body)?)
        }
    }
}

pub mod list_records {

    use chrono::prelude::*;
    use std::marker::PhantomData;

    use crate::{
        err,
        ext::NodeExt,
        metadata,
        Client
    };

    #[derive(Debug, serde::Serialize)]
    pub struct ResumptionToken {
        value: Option<String>,
        complete_list_size: u64,
        cursor: u64
    }

    #[derive(Debug, serde::Serialize)]
    pub struct ListRecords<T, U> {
        pub response_date: DateTime<Utc>,
        pub records: Vec<metadata::Record<U>>,
        pub resumption_token: Option<ResumptionToken>,
        #[serde(skip_serializing)]
        marker: PhantomData<T>
    }

    #[derive(Default, Debug, serde::Serialize)]
    pub struct Params {
        pub from: Option<DateTime<Utc>>,
        pub until: Option<DateTime<Utc>>,
        pub set: Option<String>
    }

    impl<T: metadata::Format> ListRecords<T, T::MetadataKind> {
        pub fn has_next(&self) -> bool {
            self.resumption_token.as_ref().and_then(|x| x.value.as_ref()).is_some()
        }

        pub async fn get_next(self, client: &Client) -> err::Result<ListRecords<T, T::MetadataKind>>
        {
            if let Some(rt) = self
                .resumption_token
                .as_ref()
                .and_then(|x| x.value.as_ref())
                .map(|x| String::from(x as &str))
            {
                Ok(client.list_records_from_resumption_token(rt).await?)
            } else {
                Err(err::internal("No more results"))
            }
        }

        fn build(text: String) -> err::Result<ListRecords<T, T::MetadataKind>> {
            let doc = roxmltree::Document::parse(&text).map_err(err::invalid_response)?;

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
                .flat_map(|x| -> err::Result<metadata::Record<T::MetadataKind>> {
                    let header = x.find_first_child("header")?;

                    let header = metadata::Header {
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

                    Ok(metadata::Record { header, metadata })
                })
                .collect();

            Ok(ListRecords { response_date, records, resumption_token, marker: PhantomData })
        }
    }

    impl Client {
        pub async fn list_records<T: metadata::Format>(
            &self,
            params: Option<Params>
        ) -> err::Result<ListRecords<T, T::MetadataKind>>
        {
            let params = params.unwrap_or_else(Params::default);

            #[derive(serde::Serialize)]
            struct VerbedParams {
                #[serde(flatten)]
                params: Params,
                #[serde(rename = "metadataPrefix")]
                metadata_prefix: &'static str,
                verb: &'static str
            }

            let mut url = self.base_url.clone();

            let params = VerbedParams { params, metadata_prefix: T::AS_STR, verb: "ListRecords" };

            url.set_query(Some(&serde_urlencoded::to_string(&params).map_err(err::internal)?));

            let body = self.inner.get(url).send().await.map_err(err::internal)?
                .text().await.map_err(err::internal)?;

            Ok(ListRecords::<T, T::MetadataKind>::build(body)?)
        }

        pub async fn list_records_from_resumption_token<T: metadata::Format>(
            &self,
            resumption_token: String
        ) -> err::Result<ListRecords<T, T::MetadataKind>>
        {
            #[derive(serde::Serialize)]
            struct VerbedParams {
                #[serde(rename = "resumptionToken")]
                resumption_token: String,
                verb: &'static str
            }

            let mut url = self.base_url.clone();

            let params = VerbedParams { resumption_token, verb: "ListRecords" };

            url.set_query(Some(&serde_urlencoded::to_string(&params).map_err(err::internal)?));

            let body = self.inner.get(url).send().await.map_err(err::internal)?
                .text().await.map_err(err::internal)?;

            Ok(ListRecords::<T, T::MetadataKind>::build(body)?)
        }

        pub async fn list_records_all<T: metadata::Format>(
            &self,
            params: Option<Params>
        ) -> err::Result<ListRecords<T, T::MetadataKind>>
        {
            let mut list_records = self.list_records::<T>(params).await?;
            while list_records.has_next() {
                let mut prev_records =
                    list_records.records.drain(..).collect::<Vec<_>>();

                list_records = list_records.get_next(self).await?;

                prev_records.extend(list_records.records.drain(..));

                list_records.records = prev_records;
            }

            Ok(list_records)
        }
    }
}

pub mod ext {

    pub use roxmltree;

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
                .ok_or_else(|| crate::err::internal(format!("No such tag: {}", tag_name.name())))
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
                .ok_or_else(|| crate::err::internal(format!("No text for: {}", tag_name.name())))
        }
    }

}
