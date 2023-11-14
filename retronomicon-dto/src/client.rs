use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid url")]
    InvalidUrl(#[from] url::ParseError),
    #[error("invalid token")]
    InvalidToken(#[from] reqwest::header::InvalidHeaderValue),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Server error: {0}\n{1}")]
    ServerError(StatusCode, String),
    #[error("json error")]
    Json(#[from] serde_json::Error),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct ClientConfig<'a> {
    pub token: Option<&'a str>,
}

macro_rules! declare_client {
    ( @url $fragment: literal ) => { $fragment };
    ( @url $fragment: ident ) => { &format!("{}", $fragment) };

    ( @url_args ) => {};
    ( @url_args $fragment: literal, $( $path_name: tt ),* ) => {
        declare_client!( @url_args $( $path_name ),* )
    };
    ( @url_args ($fragment: ident: $fragment_ty: ty) $( $path_name: tt ),* ) => {
        $fragment: $fragment_ty,
        declare_client!( @url_args $( $path_name ),* )
    };

    (
        $(
            $(#[$fattr:meta])*
            $method: ident $fname: ident(
                (
                    $url: literal $(,)?
                    $( $path_name: ident: $path_type: ty ),*
                    $(,)?
                )
                $(, @query $query_name: ident: $query_type: ty )*
                $(, @body $body_name: ident: $body_type: ty )*
                $(, @file $file_name: ident )*
                $(,)?
            ) -> $rtype: ty;
        )*
    ) => {
        $(
        pub async fn $fname(
            &self,
            $( $path_name: $path_type, )*
            $( $query_name: $query_type ),*
            $( $body_name: $body_type ),*
            $( $file_name: &std::path::Path ),*
        ) -> Result<$rtype, super::Error> {
            let request = self.1
                . $method (self.url( &format!($url) ))
                $(.query( $query_name ))*
                $(.json( $body_name ))*
            ;

            $(
                let path = $file_name.to_path_buf();
                let file_name = path
                    .file_name()
                    .map(|filename| filename.to_string_lossy().into_owned());
                let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
                let mime = mime_guess::from_ext(ext).first_or_octet_stream();
                let file = std::fs::read(path)?;
                let field = reqwest::multipart::Part::bytes(file).mime_str(&mime.to_string()).unwrap();

                let field = if let Some(file_name) = file_name {
                    field.file_name(file_name)
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "File name not found",
                    ).into());
                };

                let form = reqwest::multipart::Form::new().part(stringify!($file_name), field);
                let request = request.multipart(form);
            )*

            let response = request
                .send()
                .await?;

            if response.status().is_success() {
                Ok(response
                    .json()
                    .await?)
            } else {
                let status = response.status();
                let body = response.text().await?;
                Err(crate::client::Error::ServerError(status, body))
            }
        }
        )*
    };
}

pub mod v1 {
    use crate::client::ClientConfig;
    use reqwest::header;
    use reqwest::{Client, Url};

    pub const BASE: &str = "/api/v1/";

    pub struct V1Client(Url, Client);

    impl V1Client {
        fn url(&self, path: &str) -> Url {
            self.0.join(BASE).unwrap().join(path).unwrap()
        }

        fn client(config: ClientConfig) -> Result<Client, reqwest::Error> {
            let mut headers = header::HeaderMap::new();
            if let Some(token) = config.token {
                let mut auth_value =
                    header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap();
                auth_value.set_sensitive(true);
                headers.insert(header::AUTHORIZATION, auth_value);
            }

            let client = Client::builder().default_headers(headers);

            client.build()
        }

        pub fn new(url_base: impl AsRef<str>, config: ClientConfig) -> Result<Self, String> {
            Ok(Self(
                Url::parse(url_base.as_ref()).map_err(|e| e.to_string())?,
                Self::client(config).map_err(|e| e.to_string())?,
            ))
        }

        declare_client! {
            get users(
                ("users"),
                @query paging: &crate::params::PagingParams,
            ) -> Vec<crate::user::UserRef>;
            get users_details(
                ("users/{id}", id: &crate::user::UserIdOrUsername<'_>),
            ) -> crate::user::UserDetails;
            put users_update(
                ("users/{id}", id: &crate::user::UserIdOrUsername<'_>),
                @body body: &crate::user::UserUpdate<'_>,
            ) -> crate::Ok;
            put me_update(
                ("me"),
                @body body: &crate::user::UserUpdate<'_>,
            ) -> crate::Ok;

            get cores(
                ("cores"),
                @query paging: &crate::params::PagingParams,
            ) -> Vec<crate::cores::CoreListItem>;
            get cores_details(
                ("cores/{id}", id: &crate::types::IdOrSlug<'_>),
            ) -> crate::cores::CoreDetailsResponse;
            post cores_create(
                ("cores"),
                @body body: &crate::cores::CoreCreateRequest<'_>,
            ) -> crate::cores::CoreCreateResponse;

            get cores_releases(
                ("cores/{id}/releases", id: &crate::types::IdOrSlug<'_>),
                @query paging: &crate::params::PagingParams,
            ) -> Vec<crate::cores::releases::CoreReleaseListItem>;
            get cores_releases_artifacts(
                (
                    "cores/{core_id}/releases/{release_id}/artifacts",
                    core_id: &crate::types::IdOrSlug<'_>,
                    release_id: i32,
                ),
                @query paging: &crate::params::PagingParams,
            ) -> Vec<crate::artifact::CoreReleaseArtifactListItem>;
            post cores_releases_create(
                ("cores/{id}/releases", id: &crate::types::IdOrSlug<'_>),
                @body body: &crate::cores::releases::CoreReleaseCreateRequest<'_>,
            ) -> crate::cores::releases::CoreReleaseCreateResponse;
            post cores_releases_artifacts_upload(
                (
                    "cores/{core_id}/releases/{release_id}/artifacts",
                    core_id: &crate::types::IdOrSlug<'_>,
                    release_id: i32,
                ),
                @file file,
            ) -> Vec<crate::artifact::ArtifactCreateResponse>;

            get games(
                ("games"),
                @query paging: &crate::games::GameListQueryParams<'_>,
            ) -> Vec<crate::games::GameListItemResponse>;
            get games_details(
                ("games/{id}", id: u32),
            ) -> crate::games::GameDetails;
            post games_create(
                ("games"),
                @body body: &crate::games::GameCreateRequest<'_>,
            ) -> crate::games::GameCreateResponse;
            put games_update(
                ("games/{id}", id: u32),
                @body body: &crate::games::GameUpdateRequest<'_>,
            ) -> crate::Ok;
        }
    }
}

pub use v1::V1Client;
