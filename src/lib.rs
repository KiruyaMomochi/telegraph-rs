//! telegraph API binding in Rust
//!
//! See https://telegra.ph/api for more information
//!
//! # Examples
//!
//! ```
//! # async fn run() -> Result<(), telegraph_rs::Error> {
//! use telegraph_rs::{Telegraph, html_to_node};
//!
//! let telegraph = Telegraph::new("test_account").create().await?;
//!
//! let page = telegraph.create_page("title", &html_to_node("<p>Hello, world</p>"), false).await?;
//! # Ok(())
//! # }
//! ```
pub mod error;
pub mod types;
pub mod utils;

pub use error::*;
use kuchikiki::{ElementData, NodeData, NodeRef, traits::TendrilSink};
pub use types::*;
pub use utils::*;

use reqwest::{
    multipart::{Form, Part},
    Client, Response,
};
use std::{collections::HashMap, fs::File, io::Read, path::Path};

pub type Result<T> = std::result::Result<T, Error>;

macro_rules! send {
    ($e:expr) => {
        $e.send().await.and_then(Response::error_for_status)
    };
}

#[derive(Debug, Default, Clone)]
pub struct AccountBuilder {
    access_token: Option<String>,
    short_name: String,
    author_name: Option<String>,
    author_url: Option<String>,
    client: Client,
}

impl AccountBuilder {
    pub fn new(short_name: &str) -> Self {
        AccountBuilder {
            short_name: short_name.to_owned(),
            ..Default::default()
        }
    }

    /// Account name, helps users with several accounts remember which they are currently using.
    ///
    /// Displayed to the user above the "Edit/Publish" button on Telegra.ph,
    ///
    /// other users don't see this name.
    pub fn short_name(mut self, short_name: &str) -> Self {
        self.short_name = short_name.to_owned();
        self
    }

    ///  Access token of the Telegraph account.
    pub fn access_token(mut self, access_token: &str) -> Self {
        self.access_token = Some(access_token.to_owned());
        self
    }

    /// Default author name used when creating new articles.
    pub fn author_name(mut self, author_name: &str) -> Self {
        self.author_name = Some(author_name.to_owned());
        self
    }

    /// Default profile link, opened when users click on the author's name below the title.
    ///
    /// Can be any link, not necessarily to a Telegram profile or channel.
    pub fn author_url(mut self, author_url: &str) -> Self {
        self.author_url = Some(author_url.to_owned());
        self
    }

    /// Client
    pub fn client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    /// If `access_token` is not set, an new account will be create.
    ///
    /// Otherwise import the existing account.
    pub async fn create(mut self) -> Result<Telegraph> {
        if self.access_token.is_none() {
            let account = Telegraph::create_account(
                &self.short_name,
                self.author_name.as_deref(),
                self.author_url.as_deref(),
            )
            .await?;
            self.access_token = Some(account.access_token.unwrap());
        }

        Ok(Telegraph {
            client: self.client,
            access_token: self.access_token.unwrap(),
            short_name: self.short_name.to_owned(),
            author_name: self.author_name.unwrap_or(self.short_name),
            author_url: self.author_url,
        })
    }

    /// Edit info of an an existing account.
    pub async fn edit(self) -> Result<Telegraph> {
        let response = send!(Client::new()
            .get("https://api.telegra.ph/editAccountInfo")
            .query(&[
                ("access_token", self.access_token.as_ref().unwrap()),
                ("short_name", &self.short_name),
                ("author_name", self.author_name.as_ref().unwrap()),
                (
                    "author_url",
                    self.author_url.as_ref().unwrap_or(&String::new()),
                ),
            ]))?;
        let json: Result<Account> = response.json::<ApiResult<Account>>().await?.into();
        let json = json?;

        Ok(Telegraph {
            client: Client::new(),
            access_token: self.access_token.unwrap(),
            short_name: json.short_name.clone().unwrap(),
            author_name: json.author_name.or(json.short_name).unwrap(),
            author_url: json.author_url,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Telegraph {
    client: Client,
    access_token: String,
    short_name: String,
    author_name: String,
    author_url: Option<String>,
}

impl Telegraph {
    /// Use this method to create a new Telegraph account or import an existing one.
    ///
    /// Most users only need one account, but this can be useful for channel administrators who would like to keep individual author names and profile links for each of their channels.
    ///
    /// On success, returns an Account object with the regular fields and an additional access_token field.
    ///
    /// ```
    /// # async fn run() -> Result<(), telegraph_rs::Error> {
    /// use telegraph_rs::Telegraph;
    ///
    /// let account = Telegraph::new("short_name")
    ///     .access_token("b968da509bb76866c35425099bc0989a5ec3b32997d55286c657e6994bbb")
    ///     .create()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(short_name: &str) -> AccountBuilder {
        AccountBuilder {
            short_name: short_name.to_owned(),
            ..Default::default()
        }
    }

    pub(crate) async fn create_account<'a, S, T>(
        short_name: &str,
        author_name: S,
        author_url: T,
    ) -> Result<Account>
    where
        T: Into<Option<&'a str>>,
        S: Into<Option<&'a str>>,
    {
        let mut params = HashMap::new();
        params.insert("short_name", short_name);
        if let Some(author_name) = author_name.into() {
            params.insert("author_name", author_name);
        }
        if let Some(author_url) = author_url.into() {
            params.insert("author_url", author_url);
        }
        let response = send!(Client::new()
            .get("https://api.telegra.ph/createAccount")
            .query(&params))?;
        response.json::<ApiResult<Account>>().await?.into()
    }

    /// Use this method to create a new Telegraph page. On success, returns a Page object.
    ///
    /// if `return_content` is true, a content field will be returned in the Page object.
    ///
    /// ```
    /// # async fn test() -> Result<(), telegraph_rs::Error> {
    /// use telegraph_rs::{Telegraph, html_to_node};
    ///
    /// let telegraph = Telegraph::new("author")
    ///     .access_token("b968da509bb76866c35425099bc0989a5ec3b32997d55286c657e6994bbb")
    ///     .create()
    ///     .await?;
    ///
    /// let page = telegraph.create_page("title", &html_to_node("<p>Hello, world!</p>"), false).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_page(
        &self,
        title: &str,
        content: &str,
        return_content: bool,
    ) -> Result<Page> {
        let response = send!(self
            .client
            .post("https://api.telegra.ph/createPage")
            .form(&[
                ("access_token", &*self.access_token),
                ("title", title),
                ("author_name", &*self.author_name),
                ("author_url", self.author_url.as_deref().unwrap_or("")),
                ("content", content),
                ("return_content", &*return_content.to_string()),
            ]))?;
        response.json::<ApiResult<Page>>().await?.into()
    }

    pub async fn create_page_doms(
        &self,
        title: &str,
        content: impl Iterator<Item = NodeRef>,
        return_content: bool) -> Result<Page> {
            let nodes = doms_to_nodes(content);
            let content = serde_json::to_string(&nodes).unwrap();
            self.create_page(title, &content, return_content).await
        }

    /// Use this method to update information about a Telegraph account.
    ///
    /// Pass only the parameters that you want to edit.
    ///
    /// On success, returns an Account object with the default fields.
    pub fn edit_account_info(self) -> AccountBuilder {
        AccountBuilder {
            access_token: Some(self.access_token),
            short_name: self.short_name,
            author_name: Some(self.author_name),
            author_url: self.author_url,
            client: self.client,
        }
    }

    /// Use this method to edit an existing Telegraph page.
    ///
    /// On success, returns a Page object.
    pub async fn edit_page(
        &self,
        path: &str,
        title: &str,
        content: &str,
        return_content: bool,
    ) -> Result<Page> {
        let response = send!(self.client.post("https://api.telegra.ph/editPage").form(&[
            ("access_token", &*self.access_token),
            ("path", path),
            ("title", title),
            ("author_name", &*self.author_name),
            ("author_url", self.author_url.as_deref().unwrap_or("")),
            ("content", content),
            ("return_content", &*return_content.to_string()),
        ]))?;
        response.json::<ApiResult<Page>>().await?.into()
    }

    /// Use this method to get information about a Telegraph account. Returns an Account object on success.
    ///
    /// Available fields: short_name, author_name, author_url, auth_url, page_count.
    pub async fn get_account_info(&self, fields: &[&str]) -> Result<Account> {
        let response = send!(self
            .client
            .get("https://api.telegra.ph/getAccountInfo")
            .query(&[
                ("access_token", &self.access_token),
                ("fields", &serde_json::to_string(fields).unwrap()),
            ]))?;
        response.json::<ApiResult<Account>>().await?.into()
    }

    /// Use this method to get a Telegraph page. Returns a Page object on success.
    pub async fn get_page(path: &str, return_content: bool) -> Result<Page> {
        let response = Client::new()
            .get(&format!("https://api.telegra.ph/getPage/{}", path))
            .query(&[("return_content", return_content.to_string())])
            .send()
            .await?
            .error_for_status()?;
        response.json::<ApiResult<Page>>().await?.into()
    }

    /// Use this method to get a list of pages belonging to a Telegraph account.
    ///
    /// Returns a PageList object, sorted by most recently created pages first.
    ///
    /// - `offset` Sequential number of the first page to be returned. (suggest: 0)
    /// - `limit` Limits the number of pages to be retrieved. (suggest: 50)
    pub async fn get_page_list(&self, offset: i32, limit: i32) -> Result<PageList> {
        let response = send!(self
            .client
            .get("https://api.telegra.ph/getPageList")
            .query(&[
                ("access_token", &self.access_token),
                ("offset", &offset.to_string()),
                ("limit", &limit.to_string()),
            ]))?;
        response.json::<ApiResult<PageList>>().await?.into()
    }

    /// Use this method to get the number of views for a Telegraph article.
    ///
    /// Returns a PageViews object on success.
    ///
    /// By default, the total number of page views will be returned.
    ///
    /// ```rust
    /// # async fn run() -> Result<(), telegraph_rs::Error> {
    /// use telegraph_rs::Telegraph;
    ///
    /// let view1 = Telegraph::get_views("Sample-Page-12-15", &vec![2016, 12]).await?;
    /// let view2 = Telegraph::get_views("Sample-Page-12-15", &vec![2019, 5, 19, 12]).await?; // year-month-day-hour
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_views(path: &str, time: &[i32]) -> Result<PageViews> {
        let params = ["year", "month", "day", "hour"]
            .iter()
            .zip(time)
            .collect::<HashMap<_, _>>();

        let response = send!(Client::new()
            .get(&format!("https://api.telegra.ph/getViews/{}", path))
            .query(&params))?;
        response.json::<ApiResult<PageViews>>().await?.into()
    }

    /// Use this method to revoke access_token and generate a new one,
    ///
    /// for example, if the user would like to reset all connected sessions,
    ///
    /// or you have reasons to believe the token was compromised.
    ///
    /// On success, returns an Account object with new access_token and auth_url fields.
    pub async fn revoke_access_token(&mut self) -> Result<Account> {
        let response = send!(self
            .client
            .get("https://api.telegra.ph/revokeAccessToken")
            .query(&[("access_token", &self.access_token)]))?;
        let json: Result<Account> = response.json::<ApiResult<Account>>().await?.into();
        if json.is_ok() {
            self.access_token = json
                .as_ref()
                .unwrap()
                .access_token
                .as_ref()
                .unwrap()
                .to_owned();
        }
        json
    }

    /// Upload files to telegraph with custom client
    #[cfg(feature = "upload")]
    pub async fn upload_with<T: Uploadable>(
        files: &[T],
        client: &Client,
    ) -> Result<Vec<ImageInfo>> {
        let mut form = Form::new();
        for (i, file) in files.iter().enumerate() {
            let part = file.part()?;
            form = form.part(i.to_string(), part);
        }
        let response = send!(client.post("https://telegra.ph/upload").multipart(form))?;

        match response.json::<UploadResult>().await? {
            UploadResult::Error { error } => Err(Error::ApiError(error)),
            UploadResult::Source(v) => Ok(v),
        }
    }

    /// Upload files to telegraph
    #[cfg(feature = "upload")]
    pub async fn upload<T: Uploadable>(files: &[T]) -> Result<Vec<ImageInfo>> {
        Self::upload_with(files, &Client::new()).await
    }
}

#[cfg(feature = "html")]
fn html_to_node_inner(node: &html_parser::Node) -> Option<Node> {
    match node {
        html_parser::Node::Text(text) => Some(Node::Text(text.to_owned())),
        html_parser::Node::Element(element) => Some(Node::NodeElement(NodeElement {
            tag: element.name.to_owned(),
            attrs: {
                (!element.attributes.is_empty()).then(|| element.attributes.clone())
            },
            children: {
                if element.children.is_empty() {
                    None
                } else {
                    element.children.iter().map(|node| html_to_node_inner(node))
                        .collect::<Option<Vec<_>>>()
                }
            },
        })),
        _ => None,
    }
}

#[cfg(feature = "upload")]
fn guess_mime<P: AsRef<Path>>(path: P) -> String {
    let mime = mime_guess::from_path(path).first_or(mime_guess::mime::TEXT_PLAIN);
    let mut s = format!("{}/{}", mime.type_(), mime.subtype());
    if let Some(suffix) = mime.suffix() {
        s.push('+');
        s.push_str(suffix.as_str());
    }
    s
}

#[cfg(feature = "upload")]
fn read_to_bytes<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    let mut file = File::open(path)?;
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Parse html to node string
///
/// ```rust
/// use telegraph_rs::html_to_node;
///
/// let node = html_to_node("<p>Hello, world</p>");
/// assert_eq!(node, r#"[{"tag":"p","children":["Hello, world"]}]"#);
/// ```
#[cfg(feature = "kuchiki")]
pub fn html_to_node(html: &str) -> String {
    let document = kuchikiki::parse_html().one(html);
    let body = document.last_child().unwrap().last_child().unwrap();
    let nodes = doms_to_nodes(body.children());
    serde_json::to_string(&nodes).unwrap()
}

#[cfg(feature = "kuchiki")]
/// Parse the iterator of dom nodes to node structure
pub fn doms_to_nodes<T>(nodes: T) -> Option<Vec<Node>>
where T: Iterator<Item = NodeRef> {
    nodes.map(|node| dom_to_node(&node))
    .collect()
}

#[cfg(feature = "kuchiki")]
/// Parse the dom node to node structure
pub fn dom_to_node(node: &NodeRef) -> Option<Node> {
    match node.data() {
        NodeData::Text(text) => Some(Node::Text(text.borrow().clone())),
        NodeData::Element(element_data) => {
            let children: Vec<Node> = node
                .children()
                .filter_map(|node| dom_to_node(&node))
                .collect();
            let children = if children.is_empty() {
                None
            } else {
                Some(children)
            };
            Some(Node::NodeElement(NodeElement {
                tag: element_data.name.local.to_string(),
                attrs: element_data_to_attribute(element_data),
                children: children,
            }))
        }
        _ => None,
    }
}

fn element_data_to_attribute(element_data: &ElementData) -> Option<HashMap<String, Option<String>>> {
    let map = &element_data.attributes.borrow().map;
    if map.is_empty() {
        return None;
    }

    let mut attrs = HashMap::new();
    map.iter()
        .filter(|(name, _attr)| {
            // FIXME: Now the key of function return type is Option<String>, we can
            // handle empty value as None.
            name.local.eq_str_ignore_ascii_case("href")
                || name.local.eq_str_ignore_ascii_case("src")
        })
        .for_each(|(name, attr)| {
            attrs.insert(name.local.to_string(), Some(attr.value.clone()));
        });

    if attrs.is_empty() {
        None
    } else {
        Some(attrs)
    }
}

#[cfg(test)]
mod tests {
    use crate::Telegraph;

    #[test]
    fn html_to_node() {
        let html = r#"<a>Text</a><p>img:<img src="https://me"></p>"#;
        println!("{}", super::html_to_node(html));
    }

    #[tokio::test]
    async fn create_and_revoke_account() {
        let result = Telegraph::create_account("sample", "a", None).await;
        println!("{:?}", result);
        assert!(result.is_ok());

        let mut telegraph = Telegraph::new("test")
            .access_token(&result.unwrap().access_token.unwrap().to_owned())
            .create()
            .await
            .unwrap();
        let result = telegraph.revoke_access_token().await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn edit_account_info() {
        let result = Telegraph::new("test")
            .access_token("d3b25feccb89e508a9114afb82aa421fe2a9712b963b387cc5ad71e58722")
            .create()
            .await
            .unwrap()
            .edit_account_info()
            .short_name("wow")
            .edit()
            .await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_account_info() {
        let result = Telegraph::new("test")
            .access_token("d3b25feccb89e508a9114afb82aa421fe2a9712b963b387cc5ad71e58722")
            .create()
            .await
            .unwrap()
            .get_account_info(&["short_name"])
            .await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn create_get_edit_page() {
        let telegraph = Telegraph::new("test")
            .access_token("d3b25feccb89e508a9114afb82aa421fe2a9712b963b387cc5ad71e58722")
            .create()
            .await
            .unwrap();
        let page = telegraph
            .create_page(
                "OVO",
                r#"[{"tag":"p","children":["Hello,+world!"]}]"#,
                false,
            )
            .await;
        println!("{:?}", page);
        assert!(page.is_ok());

        let page = Telegraph::get_page(&page.unwrap().path, true).await;
        println!("{:?}", page);
        assert!(page.is_ok());

        let page = telegraph
            .edit_page(
                &page.unwrap().path,
                "QAQ",
                r#"[{"tag":"p","children":["Goodbye,+world!"]}]"#,
                false,
            )
            .await;
        println!("{:?}", page);
        assert!(page.is_ok());
    }

    #[tokio::test]
    async fn get_page_list() {
        let telegraph = Telegraph::new("test")
            .access_token("d3b25feccb89e508a9114afb82aa421fe2a9712b963b387cc5ad71e58722")
            .create()
            .await
            .unwrap();
        let page_list = telegraph.get_page_list(0, 3).await;
        println!("{:?}", page_list);
        assert!(page_list.is_ok());
    }

    #[tokio::test]
    async fn get_views() {
        let views = Telegraph::get_views("Sample-Page-12-15", &vec![2016, 12]).await;
        println!("{:?}", views);
        assert!(views.is_ok());
    }

    #[ignore]
    #[tokio::test]
    #[cfg(feature = "upload")]
    async fn upload() {
        let images = Telegraph::upload(&vec!["1.jpeg", "2.jpeg"]).await;
        println!("{:?}", images);
        assert!(images.is_ok());
    }
}
