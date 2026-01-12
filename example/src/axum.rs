//! This is a pretty straightforward TODO app using the Axum web framework.
//! You can add TODOs to your list but only if they meet certain criteria.
//!
//! To run it:
//!
//! ```shell
//! cargo run
//! ```
//!
//! Add a TODO (HTTP 202):
//!
//! ```shell
//! curl -w "%{http_code}" -XPOST -H "content-type: application/json" \
//!   http://localhost:8080/todos -d '{"kind":"work","text":"Learn more Rust"}'
//! ```
//!
//! Fetch the current TODOs:
//!
//! ```shell
//! curl http://localhost:8080/todos
//! ```
//!
//! Add another TODO (HTTP 400):
//!
//! ```shell
//! curl -w "%{http_code}" -XPOST -H "content-type: application/json" \
//!   http://localhost:8080/todos -d '{"kind":"home","text":"Learn more Rust"}'
//! ```

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use cel::{Context, Program, Value};
use serde::{Deserialize, Serialize};

// Policies dictating which text TODOs may contain
const WORK_TODO_POLICY: &str = "!text.contains('TV')"; // Don't watch TV at work!
const HOME_TODO_POLICY: &str = "!text.contains('Rust')"; // Don't hack on Rust at home!

// TODOs carry some text and have a kind
#[derive(Clone, Deserialize, Serialize)]
struct Todo {
    text: String,
    kind: TodoKind,
}

// TODOs are either for work or for home
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum TodoKind {
    Work,
    Home,
}

// POST a new TODO to the list
async fn add_todo(
    State(AppContext { todos, decider }): State<AppContext>,
    Json(todo): Json<Todo>,
) -> impl IntoResponse {
    // Use the policy decider to see if the TODO is allowed
    match decider.todo_is_allowed(&todo) {
        Ok(is_allowed) => {
            // If not, throw HTTP 400
            if !is_allowed {
                return StatusCode::BAD_REQUEST;
            }

            // If allowed, add it to the TODOs and return HTTP 202
            let mut todos = todos.lock().unwrap();
            todos.push(todo);
            StatusCode::ACCEPTED
        }
        // If there's an error return an HTTP 500
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// GET the current list of TODOs
async fn list_todos(State(AppContext { todos, .. }): State<AppContext>) -> impl IntoResponse {
    Json(todos.lock().unwrap().clone())
}

// The policy engine for our TODOs app
struct PolicyDecider(Context<'static>);

impl PolicyDecider {
    // Start with a wrapper around the default Context
    fn new() -> Self {
        Self(Context::default())
    }

    // Determine whether a given TODO is allowed
    fn todo_is_allowed(&self, todo: &Todo) -> Result<bool, TodosError> {
        // Create a new mutable context out of the root context
        let mut ctx = self.0.new_inner_scope();
        // Add the TODO's text as a variable so that it can be part of the expression
        ctx.add_variable_from_value("text", todo.text.clone());

        // Which policy to enforce depends on the kind of TODO
        let policy = match todo.kind {
            TodoKind::Home => HOME_TODO_POLICY,
            TodoKind::Work => WORK_TODO_POLICY,
        };

        // Compile the program
        let program = Program::compile(policy)?;

        // Execute the program and either return a Boolean or the TODO is
        // considered invalid
        match program.execute(&ctx)? {
            Value::Bool(b) => Ok(b),
            _ => Err(TodosError::Invalid),
        }
    }
}

// Custom error type
#[derive(Debug, thiserror::Error)]
enum TodosError {
    #[error("CEL execution error: {0}")]
    Execution(#[from] cel::ExecutionError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("CEL parse error: {0}")]
    Parse(#[from] cel::ParseErrors),
    #[error("invalid TODO")]
    Invalid,
}

// The state attached to the HTTP router
#[derive(Clone)]
struct AppContext {
    todos: Arc<Mutex<Vec<Todo>>>,
    decider: Arc<PolicyDecider>,
}

#[tokio::main]
async fn main() -> Result<(), TodosError> {
    let ctx = AppContext {
        todos: Arc::new(Mutex::new(Vec::new())),
        decider: Arc::new(PolicyDecider::new()),
    };

    let app = Router::new()
        .route("/todos", get(list_todos).post(add_todo))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    Ok(axum::serve(listener, app).await?)
}
