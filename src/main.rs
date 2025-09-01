use clap::Parser;
use color_eyre::eyre::Result;
use log::{error, info, warn};
use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser as SqlParser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_postgres::{Client, NoTls};

#[derive(Parser, Debug)]
#[command(name = "vibedb")]
#[command(about = "A Postgres proxy with built-in safety features")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:6543")]
    listen: SocketAddr,

    #[arg(long)]
    db_url: String,

    #[arg(long, default_value = "true")]
    strict: bool,

    #[arg(long, default_value = "500")]
    max_rows: i64,
}

#[derive(Clone)]
struct ProxyConfig {
    db_url: String,
    strict: bool,
    max_rows: i64,
}

enum QueryAction {
    Allow,
    Block(String),
    CheckRows(String),
}

struct QueryAnalyzer {}

impl QueryAnalyzer {
    fn new() -> Self {
        Self {}
    }

    fn analyze_query(&self, query: &str) -> QueryAction {
        // Honeytoken detection
        if query.to_lowercase().contains("_vibedb_canary") {
            return QueryAction::Block("honeytoken table access detected".to_string());
        }

        let dialect = PostgreSqlDialect {};
        let ast = match SqlParser::parse_sql(&dialect, query) {
            Ok(ast) => ast,
            Err(_) => return QueryAction::Allow, // Allow unparseable queries
        };

        for statement in &ast {
            match statement {
                Statement::Drop { .. } => {
                    return QueryAction::Block("DROP statement blocked".to_string());
                }
                Statement::Truncate { .. } => {
                    return QueryAction::Block("TRUNCATE statement blocked".to_string());
                }
                Statement::Delete { .. } => {
                    // Check if DELETE has WHERE clause by parsing the query string
                    if !query.to_uppercase().contains(" WHERE ") {
                        return QueryAction::Block(
                            "DELETE without WHERE clause blocked".to_string(),
                        );
                    }
                    return QueryAction::CheckRows(self.create_count_query_from_delete(query));
                }
                Statement::Update { selection, .. } => {
                    // Check if UPDATE has WHERE clause
                    if selection.is_some() {
                        return QueryAction::CheckRows(self.create_count_query_from_update(query));
                    }
                }
                _ => {}
            }
        }

        QueryAction::Allow
    }

    fn create_count_query_from_delete(&self, delete_query: &str) -> String {
        let table_name = self.extract_table_name_from_query(delete_query, "DELETE FROM");
        let where_clause = if delete_query.to_uppercase().contains(" WHERE ") {
            let parts: Vec<&str> = delete_query.splitn(2, " WHERE ").collect();
            if parts.len() == 2 {
                format!(" WHERE {}", parts[1])
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        format!("SELECT COUNT(*) FROM {}{}", table_name, where_clause)
    }

    fn create_count_query_from_update(&self, update_query: &str) -> String {
        let table_name = self.extract_table_name_from_query(update_query, "UPDATE");
        let where_clause = if update_query.to_uppercase().contains(" WHERE ") {
            let parts: Vec<&str> = update_query.splitn(2, " WHERE ").collect();
            if parts.len() == 2 {
                format!(" WHERE {}", parts[1])
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        format!("SELECT COUNT(*) FROM {}{}", table_name, where_clause)
    }

    fn extract_table_name_from_query(&self, query: &str, prefix: &str) -> String {
        let upper_query = query.to_uppercase();
        let upper_prefix = prefix.to_uppercase();

        if let Some(start_pos) = upper_query.find(&upper_prefix) {
            let after_prefix = &query[start_pos + prefix.len()..];
            let table_part = after_prefix.trim_start();

            let table_name = table_part
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .trim_matches(';');

            table_name.to_string()
        } else {
            "unknown".to_string()
        }
    }
}

struct PostgresProxy {
    config: ProxyConfig,
    analyzer: QueryAnalyzer,
}

impl PostgresProxy {
    fn new(config: ProxyConfig) -> Self {
        let analyzer = QueryAnalyzer::new();
        Self { config, analyzer }
    }

    async fn handle_client(&self, client_stream: TcpStream) -> Result<()> {
        info!("new client connection established");

        // Create a connection for query analysis
        let (db_client, connection) = tokio_postgres::connect(&self.config.db_url, NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("database connection error: {}", e);
            }
        });

        // Connect directly to the database for transparent proxying
        let db_stream = match self.connect_to_database().await {
            Ok(stream) => stream,
            Err(e) => {
                error!("failed to connect to database: {}", e);
                return Err(e);
            }
        };

        // Start bidirectional forwarding with query interception
        self.handle_bidirectional_proxy(client_stream, db_stream, db_client)
            .await
    }

    async fn connect_to_database(&self) -> Result<TcpStream> {
        let host_port = self.extract_host_port(&self.config.db_url)?;
        let stream = TcpStream::connect(&host_port).await?;
        Ok(stream)
    }

    fn extract_host_port(&self, db_url: &str) -> Result<String> {
        if let Ok(parsed_url) = url::Url::parse(db_url) {
            let host = parsed_url.host_str().unwrap_or("localhost");
            let port = parsed_url.port().unwrap_or(5432);
            Ok(format!("{}:{}", host, port))
        } else {
            Ok("localhost:5432".to_string())
        }
    }

    async fn handle_bidirectional_proxy(
        &self,
        mut client_stream: TcpStream,
        mut db_stream: TcpStream,
        db_client: Client,
    ) -> Result<()> {
        let mut client_buffer = vec![0; 8192];
        let mut db_buffer = vec![0; 8192];

        loop {
            tokio::select! {
                // Client to DB traffic (intercept queries)
                result = client_stream.read(&mut client_buffer) => {
                    match result {
                        Ok(0) => break,
                        Ok(n) => {
                            let data = &client_buffer[..n];

                            if let Some(query) = self.extract_query_from_message(data) {
                                info!("intercepted query: {}", query);

                                match self.analyzer.analyze_query(&query) {
                                    QueryAction::Allow => {
                                        info!("[ALLOW] {}", query);
                                        if let Err(e) = db_stream.write_all(data).await {
                                            error!("failed to forward to database: {}", e);
                                            break;
                                        }
                                    }
                                    QueryAction::Block(reason) => {
                                        warn!("[BLOCK] {} → {}", query, reason);
                                        let error_response = self.create_simple_error_response(&reason);
                                        if let Err(e) = client_stream.write_all(&error_response).await {
                                            error!("failed to send error response: {}", e);
                                        }
                                        continue;
                                    }
                                    QueryAction::CheckRows(count_query) => {
                                        match self.check_row_count(&db_client, &count_query).await {
                                            Ok(row_count) => {
                                                if row_count > self.config.max_rows {
                                                    let reason = format!("would affect {} rows (limit {})", row_count, self.config.max_rows);
                                                    warn!("[BLOCK] {} → {}", query, reason);
                                                    let error_response = self.create_simple_error_response(&reason);
                                                    if let Err(e) = client_stream.write_all(&error_response).await {
                                                        error!("failed to send error response: {}", e);
                                                    }
                                                    continue;
                                                } else {
                                                    info!("[snapshot] would take backup here");
                                                    info!("[ALLOW] {} → {} rows", query, row_count);
                                                    if let Err(e) = db_stream.write_all(data).await {
                                                        error!("failed to forward to database: {}", e);
                                                        break;
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                error!("failed to check row count: {}", e);
                                                let error_response = self.create_simple_error_response("Internal error checking row count");
                                                if let Err(e) = client_stream.write_all(&error_response).await {
                                                    error!("Failed to send error response: {}", e);
                                                }
                                                continue;
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Forward non-query messages directly (handshake, etc.)
                                if let Err(e) = db_stream.write_all(data).await {
                                    error!("Failed to forward to database: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("failed to read from client: {}", e);
                            break;
                        }
                    }
                }

                // DB to client traffic (forward all responses)
                result = db_stream.read(&mut db_buffer) => {
                    match result {
                        Ok(0) => break,
                        Ok(n) => {
                            let data = &db_buffer[..n];
                            if let Err(e) = client_stream.write_all(data).await {
                                error!("failed to forward to client: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("failed to read from database: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        info!("proxy connection closed");
        Ok(())
    }

    fn extract_query_from_message(&self, data: &[u8]) -> Option<String> {
        // Simple query message starts with 'Q'
        if data.len() > 5 && data[0] == b'Q' {
            let query_bytes = &data[5..];
            if let Some(null_pos) = query_bytes.iter().position(|&b| b == 0) {
                if let Ok(query) = String::from_utf8(query_bytes[..null_pos].to_vec()) {
                    return Some(query.trim().to_string());
                }
            }
        }
        None
    }

    async fn check_row_count(&self, client: &Client, count_query: &str) -> Result<i64> {
        let rows = client.query(count_query, &[]).await?;
        if let Some(row) = rows.first() {
            let count: i64 = row.get(0);
            Ok(count)
        } else {
            Ok(0)
        }
    }

    fn create_simple_error_response(&self, message: &str) -> Vec<u8> {
        let mut response = Vec::new();

        response.push(b'E'); // Error message

        let error_fields = format!("SERROR\0C42000\0M{}\0\0", message);

        let msg_len = (error_fields.len() + 4) as u32;
        response.extend_from_slice(&msg_len.to_be_bytes());
        response.extend_from_slice(error_fields.as_bytes());

        response.push(b'Z'); // ReadyForQuery
        response.extend_from_slice(&5u32.to_be_bytes()); // Length
        response.push(b'I'); // Idle

        response
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::init();
    let args = Args::parse();

    info!("starting VibeDB Postgres Proxy");
    info!("listening on: {}", args.listen);
    info!("database URL: {}", args.db_url);
    info!("strict mode: {}", args.strict);
    info!("max rows: {}", args.max_rows);

    let proxy_url = if let Ok(mut parsed_url) = url::Url::parse(&args.db_url) {
        parsed_url
            .set_host(Some(&args.listen.ip().to_string()))
            .ok();
        parsed_url.set_port(Some(args.listen.port())).ok();
        parsed_url.to_string()
    } else {
        format!(
            "postgres://user:pass@{}:{}/<database>",
            args.listen.ip(),
            args.listen.port()
        )
    };

    info!("connect through proxy: {}", proxy_url);

    let config = ProxyConfig {
        db_url: args.db_url,
        strict: args.strict,
        max_rows: args.max_rows,
    };

    let proxy = Arc::new(PostgresProxy::new(config));
    let listener = TcpListener::bind(args.listen).await?;

    info!("proxy server started successfully");

    loop {
        let (client_stream, addr) = listener.accept().await?;
        info!("new connection from: {}", addr);

        let proxy_clone = Arc::clone(&proxy);
        tokio::spawn(async move {
            if let Err(e) = proxy_clone.handle_client(client_stream).await {
                error!("error handling client {}: {}", addr, e);
            }
        });
    }
}
