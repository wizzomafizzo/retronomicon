use crate::fairings::config::{JwtKeys, RetronomiconConfig};
use jsonwebtoken::{DecodingKey, EncodingKey};
use retronomicon_db::models::{User, UserTeam};
use retronomicon_db::Db;
use retronomicon_dto as dto;
use rocket::http::{Cookie, CookieJar, Status};
use rocket::outcome::{IntoOutcome, Outcome};
use rocket::{request, warn, Request, State};
use rocket_okapi::OpenApiFromRequest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ops::{Deref, DerefMut};

/// A user that is part of the root team.
#[derive(Debug, Clone, Serialize, Deserialize, OpenApiFromRequest)]
pub struct RootUserGuard {
    pub id: i32,
}

#[rocket::async_trait]
impl<'r> request::FromRequest<'r> for RootUserGuard {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let config = request
            .rocket()
            .state::<RetronomiconConfig>()
            .expect("No retronomicon config");

        let mut db = request
            .guard::<Db>()
            .await
            .expect("Database connection guard");
        let user = match request.guard::<UserGuard>().await {
            Outcome::Success(user) => user,
            Outcome::Forward(_) => return Outcome::Forward(Status::Unauthorized),
            Outcome::Error((status, _)) => return Outcome::Error((status, "Unauthorized".into())),
        };

        let result = UserTeam::user_is_in_team(&mut db, user.id, config.root_team_id)
            .await
            .map_err(|e| e.to_string());
        match result {
            Ok(true) => Outcome::Success(RootUserGuard { id: user.id }),
            Ok(false) => Outcome::Forward(Status::Unauthorized),
            Err(e) => Outcome::Error((Status::InternalServerError, e)),
        }
    }
}

/// A user that went through the signed up process and has a username.
#[derive(Debug, Clone, Serialize, Deserialize, OpenApiFromRequest)]
pub struct AuthenticatedUserGuard {
    inner: UserGuard,
}

#[rocket::async_trait]
impl<'r> request::FromRequest<'r> for AuthenticatedUserGuard {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        UserGuard::from_request(request).await.and_then(|user| {
            if let Some(user) = user.into() {
                Outcome::Success(user)
            } else {
                Outcome::Forward(Status::Unauthorized)
            }
        })
    }
}

impl From<AuthenticatedUserGuard> for dto::user::UserIdOrUsername<'static> {
    fn from(user: AuthenticatedUserGuard) -> Self {
        Self::Id(user.id)
    }
}

impl Deref for AuthenticatedUserGuard {
    type Target = UserGuard;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for AuthenticatedUserGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl TryFrom<UserGuard> for AuthenticatedUserGuard {
    type Error = &'static str;

    fn try_from(value: UserGuard) -> Result<Self, Self::Error> {
        if value.username.is_some() {
            Ok(Self { inner: value })
        } else {
            Err("User is not authenticated")
        }
    }
}

impl AuthenticatedUserGuard {
    pub fn into_inner(self) -> UserGuard {
        self.inner
    }

    pub async fn into_model(self, db: &mut Db) -> Result<User, (Status, String)> {
        self.inner.into_model(db).await
    }
}

/// A potentially non-fully signed-up user for the website.
#[derive(Clone, Debug, Serialize, Deserialize, OpenApiFromRequest)]
pub struct UserGuard {
    pub id: i32,
    pub username: Option<String>,

    pub exp: i64,
}

#[rocket::async_trait]
impl<'r> request::FromRequest<'r> for UserGuard {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<UserGuard, Self::Error> {
        fn validate_exp(user: UserGuard) -> request::Outcome<UserGuard, String> {
            if chrono::Utc::now().timestamp() > user.exp {
                Outcome::Forward(Status::Unauthorized)
            } else {
                Outcome::Success(user)
            }
        }

        // Check cookies.
        let cookies = request
            .guard::<&CookieJar<'_>>()
            .await
            .expect("request cookies");
        if let Some(cookie) = cookies.get_private("auth") {
            let json: Result<UserGuard, _> =
                serde_json::from_str(cookie.value()).map_err(|e| e.to_string());

            return json
                .or_error(Status::Unauthorized)
                .and_then(validate_exp)
                .and_then(|user: UserGuard| {
                    user.update_cookie(cookies);
                    Outcome::Success(user)
                });
        }

        let jwt_secret = match request.guard::<&State<JwtKeys>>().await {
            Outcome::Success(secret) => &secret.decoding,
            Outcome::Forward(_) => return Outcome::Forward(Status::Unauthorized),
            Outcome::Error((status, _)) => return Outcome::Error((status, "Unauthorized".into())),
        };

        // Check JWT from the headers.
        request
            .headers()
            .get_one("Authorization")
            .ok_or("Unauthorized".to_string())
            .and_then(|key| UserGuard::decode_jwt(key, jwt_secret).map_err(|e| e.to_string()))
            .or_error(Status::Unauthorized)
            .and_then(validate_exp)
    }
}

impl From<UserGuard> for Option<AuthenticatedUserGuard> {
    fn from(value: UserGuard) -> Self {
        AuthenticatedUserGuard::try_from(value).ok()
    }
}

impl<'a> From<&UserGuard> for Cookie<'a> {
    fn from(user: &UserGuard) -> Self {
        Cookie::build(("auth", serde_json::to_string(user).unwrap()))
            .same_site(rocket::http::SameSite::Lax)
            .build()
    }
}

impl From<UserGuard> for Option<dto::user::UserRef> {
    fn from(user: UserGuard) -> Self {
        user.username.map(|username| dto::user::UserRef {
            id: user.id,
            username,
        })
    }
}

impl From<UserGuard> for dto::user::UserIdOrUsername<'static> {
    fn from(user: UserGuard) -> Self {
        Self::Id(user.id)
    }
}

impl UserGuard {
    pub fn new(id: i32, username: Option<String>, exp: i64) -> Result<Self, &'static str> {
        if exp < chrono::Utc::now().timestamp() {
            return Err("Invalid expiry");
        }
        if let Some(ref n) = username {
            dto::user::Username::new(n)?; // Validate username.
        }

        Ok(Self::new_unchecked(id, username, exp))
    }

    pub fn new_unchecked(id: i32, username: Option<String>, exp: i64) -> Self {
        Self { id, username, exp }
    }

    pub fn set_expiry(&mut self, expiry: i64) {
        self.exp = expiry;
    }

    pub async fn into_model(self, db: &mut Db) -> Result<User, (Status, String)> {
        User::from_id(db, self.id)
            .await
            .map_err(|e| (Status::InternalServerError, e.to_string()))
    }

    pub fn from_model(user: User) -> Self {
        Self::new_unchecked(user.id, user.username, default_expiration_())
    }

    pub async fn from_db(db: &mut Db, id: i32) -> Result<Self, (Status, String)> {
        User::from_id(db, id)
            .await
            .map(Self::from_model)
            .map_err(|e| (Status::Unauthorized, e.to_string()))
    }

    /// Create a new user or select an existing one. This should only be used
    /// from an OAuth provider.
    pub async fn login_from_auth(
        db: &mut Db,
        username: Option<String>,
        email: &str,
        auth_provider: String,
        avatar_url: Option<String>,
    ) -> Result<(bool, User, Self), (Status, String)> {
        // Set username to None if it doesn't validate.
        let username = username.as_deref().and_then(|u| {
            dto::user::Username::new(u)
                .map(|u| u.into_inner())
                .map_err(|e| {
                    warn!("login_from_auth: invalid username: {}", e);
                    e
                })
                .ok()
        });

        let maybe_user = User::from_auth(db, email, &auth_provider)
            .await
            .map_err(|e| (Status::Unauthorized, e.to_string()))?;

        if let Some(u) = maybe_user {
            return Ok((false, u.clone(), Self::from_model(u)));
        }

        let user = User::create(
            db,
            username.as_deref(),
            None,
            avatar_url.as_deref(),
            email,
            Some(&auth_provider),
            None,
            json!({}),
            json!({}),
        )
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;

        Ok((true, user.clone(), Self::from_model(user)))
    }

    pub async fn login_from_password(
        db: &mut Db,
        email: &str,
        password: &str,
        pepper: &[u8],
    ) -> Result<(User, Self), (Status, String)> {
        User::from_email(db, email, password, pepper)
            .await
            .map(|user| (user.clone(), Self::from_model(user)))
            .map_err(|e| (Status::Unauthorized, e.to_string()))
    }

    pub async fn update(
        &self,
        db: &mut Db,
        form: dto::user::UserUpdate<'_>,
    ) -> Result<(), (Status, String)> {
        let user = self.clone().into_model(db).await?;
        user.update(db, form)
            .await
            .map_err(|e| (Status::InternalServerError, e.to_string()))
    }

    pub fn update_cookie(&self, cookies: &CookieJar<'_>) {
        // Set a private cookie with the user's name, and redirect to the home page.
        cookies.add_private(self);
    }

    pub fn decode_jwt(token: &str, key: &DecodingKey) -> Result<Self, jsonwebtoken::errors::Error> {
        let token = token.trim_start_matches("Bearer ").trim();
        jsonwebtoken::decode(
            token,
            key,
            &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS512),
        )
        .map(|token| token.claims)
    }

    pub fn create_jwt(mut self, key: &EncodingKey) -> Result<String, jsonwebtoken::errors::Error> {
        let expiration = default_expiration_();
        self.set_expiry(expiration);

        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS512),
            &self,
            key,
        )
    }

    pub fn remove_cookie(&self, cookies: &CookieJar) {
        cookies.remove_private("auth");
    }
}

fn default_expiration_() -> i64 {
    chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(7))
        .expect("Invalid timestamp")
        .timestamp()
}
