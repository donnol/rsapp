#![feature(test)]
extern crate test;

use std::{fmt::Display, fmt::Formatter, fmt::Result, ops::Add, time::Duration};

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use config::Config;
use log::info;
use serde_derive::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::{signal, time::sleep};

#[derive(Deserialize, Debug, Clone)]
struct Conf {
    name: String,
    postgres: Pg,
}

#[derive(Deserialize, Debug, Clone)]
struct Pg {
    dsn: String,
}

impl Display for Conf {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "name: {}, postgres: {}", self.name, self.postgres)
    }
}

impl Display for Pg {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "dsn: *")
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    Server {
        #[arg(short, long)]
        port: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.cmd {
        Commands::Server { port } => serve(&port.unwrap_or("9009".to_owned())).await,
    };
}

async fn serve(port: &str) {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let settings = Config::builder()
        .add_source(config::File::with_name("config.toml"))
        .build()
        .unwrap();

    // Print out our settings (as a HashMap)
    let conf = settings.try_deserialize::<Conf>().unwrap();
    println!("{}, {}", conf, conf.name);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&conf.postgres.dsn)
        .await
        .unwrap();

    // Make a simple query to return the given parameter (use a question mark `?` instead of `$1` for MySQL/MariaDB)
    let row: (i64,) = sqlx::query_as("SELECT $1")
        .bind(150_i64)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0, 150);

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/longtime", get(long_time_request))
        // `POST /users` goes to `create_user`
        .route("/users", post(create_user))
        .with_state(pool);

    info!("port: {}", port);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:".to_owned().add(port))
        .await
        .unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

// for graceful shutdown. When running this request, ctrl+c will wait this request finish.
async fn long_time_request() -> &'static str {
    sleep(Duration::from_secs(10)).await;

    "Long time request."
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let client = reqwest::Client::new();
        let res = client
            .post("http://localhost:9009/users")
            .header("Content-Type", "application/json")
            .body("{\"username\": \"jd\"}")
            .send();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let r = rt.block_on(res);
        let data = rt.block_on(r.unwrap().bytes());

        assert_eq!(
            data.ok().unwrap(),
            "{\"id\":1337,\"username\":\"hello world from pg\"}"
        );
    }

    #[bench]
    fn bench_create_user(b: &mut test::Bencher) {
        b.iter(|| it_works());
    }
}

async fn create_user(
    State(pool): State<PgPool>,
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    // insert your application logic here
    let mut user = User {
        id: 1337,
        username: payload.username,
    };

    let mut tx = pool.begin().await.unwrap();
    let name = sqlx::query_scalar::<_, String>("select 'hello world from pg'")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error);
    tx.commit().await.unwrap();
    info!("{:?}", name);
    user.username = name.unwrap();

    let users = vec![
        User {
            id: 1,
            username: "ja".to_string(),
        },
        User {
            id: 2,
            username: "jb".to_string(),
        },
    ];
    println!(
        "ids: {:?}",
        users.iter().map(|item| item.id).collect::<Vec<_>>()
    );

    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::CREATED, Json(user))
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct CreateUser {
    username: String,
}

// the output to our `create_user` handler
#[derive(Serialize)]
struct User {
    id: u64,
    username: String,
}

/// Utility function for mapping any error into a `500 Internal Server Error`
/// response.
fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: std::error::Error,
{
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
