use crate::db::run_migrations;
use crate::routes::v1;
use rocket::figment::value::{Map, Value};
use rocket::figment::{map, Provider};
use rocket::response::status::NoContent;
use rocket::{get, http::Status, routes};
use rocket_db_pools::Database;
use rocket_oauth2::OAuth2;
use rocket_okapi::rapidoc::{make_rapidoc, GeneralConfig, HideShowConfig, RapiDocConfig};
use rocket_okapi::settings::UrlObject;
use rocket_okapi::swagger_ui::{make_swagger_ui, SwaggerUIConfig};
use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

mod db;
mod fairings;
mod guards;
mod models;
mod schema;
mod types;

mod routes;
mod utils;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RetronomiconConfig {
    pub base_url: String,
    pub root_team: Vec<String>,
    pub root_team_id: i32,
}

#[get("/health")]
async fn health_handler() -> Result<NoContent, Status> {
    Ok(NoContent)
}

fn database_config() -> impl Provider {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool_size: u32 = str::parse(&env::var("DATABASE_POOL_SIZE").unwrap_or("10".to_string()))
        .expect("Invalid DATABASE_POOL_SIZE");

    let db: Map<_, Value> = map! {
        "url" => db_url.into(),
        "pool_size" => pool_size.into(),
    };
    ("databases", map!["retronomicon_db" => db])
}

fn oauth_config() -> impl Provider {
    let mut oauth = BTreeMap::new();
    for (k, v) in env::vars() {
        if k.starts_with("ROCKET_OAUTH_") {
            let mut parts = k.splitn(4, '_');
            parts.next();
            parts.next();

            let provider = parts.next().unwrap();
            let key = parts.next().unwrap();
            let value = oauth
                .entry(provider.to_lowercase())
                .or_insert_with(BTreeMap::<String, String>::new);
            value.insert(key.to_lowercase(), v);
        }
    }

    ("oauth", oauth)
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    // The order is first, local environment variables, then global ones, then
    // only use development if in debug mode.
    dotenvy::from_filename(".env.local").ok();
    dotenvy::dotenv().ok();

    #[cfg(debug_assertions)]
    dotenvy::from_filename(".env.development").ok();

    run_migrations();

    let figment = rocket::Config::figment()
        .merge(database_config())
        .merge(oauth_config());

    let v = figment.find_value("secret_key").unwrap();
    env::set_var(
        "JWT_SECRET",
        v.into_string().expect("Could not find the secret_key."),
    );

    let static_root = figment
        .find_value("static_root")
        .ok()
        .and_then(|v| v.into_string())
        .unwrap_or_else(|| {
            env::var("STATIC_ROOT").ok().unwrap_or_else(|| {
                env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("static")
                    .to_string_lossy()
                    .to_string()
            })
        });

    let prometheus = rocket_prometheus::PrometheusMetrics::new();

    let rocket = rocket::custom(figment);
    let rocket = rocket
        // The health endpoint.
        .mount("/", routes![health_handler])
        .mount("/api", routes::routes())
        // The v1 actual API endpoints.
        .mount("/api/v1", v1::routes())
        .mount(
            "/api/swagger",
            make_swagger_ui(&SwaggerUIConfig {
                url: "/api/v1/openapi.json".to_owned(),
                ..Default::default()
            }),
        )
        .mount(
            "/api/rapidoc/",
            make_rapidoc(&RapiDocConfig {
                general: GeneralConfig {
                    spec_urls: vec![UrlObject::new("General", "../openapi.json")],
                    ..Default::default()
                },
                hide_show: HideShowConfig {
                    allow_spec_url_load: false,
                    allow_spec_file_load: false,
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .mount("/metrics", prometheus.clone())
        .mount(
            "/",
            rocket::fs::FileServer::from(PathBuf::from(static_root)),
        )
        .attach(db::RetronomiconDb::init())
        .attach(prometheus)
        .attach(OAuth2::<routes::auth::GitHubUserInfo>::fairing("github"))
        .attach(OAuth2::<routes::auth::GoogleUserInfo>::fairing("google"))
        .attach(fairings::cors::Cors)
        .attach(rocket::fairing::AdHoc::config::<RetronomiconConfig>());

    rocket.launch().await?;

    Ok(())
}
