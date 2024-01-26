use std::{fmt::Display, ops::Add};

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use config::Config;
use log::info;
use serde_derive::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;

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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "name: {}, postgres: {}", self.name, self.postgres)
    }
}

impl Display for Pg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
        // 使用#[arg(...)]可以把Arguments改为Options
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
        // Add in `./Settings.toml`
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
        // `POST /users` goes to `create_user`
        .route("/users", post(create_user));

    info!("port: {}", port);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:".to_owned().add(port))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn create_user(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    // insert your application logic here
    let user = User {
        id: 1337,
        username: payload.username,
    };

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
