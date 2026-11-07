//! A TODO app using the Axum web framework, where every TODO must satisfy
//! a policy written in CEL. Each kind of TODO has its own policy, which can
//! be replaced while the app runs.
//!
//! It shows what to share across requests and what not to:
//!
//! - the [`Env`], with the functions policies may call, is built once, at
//!   startup;
//! - a policy is compiled into a [`Program`] once, when it's set;
//! - each request only builds a [`Context`] of its own, holding its input,
//!   and executes the policy's `Program` with it.
//!
//! To run it:
//!
//! ```shell
//! cargo run -p example --bin example-axum --features axum
//! ```
//!
//! Add a TODO (HTTP 202):
//!
//! ```shell
//! curl -w "%{http_code}" -XPOST -H "content-type: application/json" -H "x-user: alice" \
//!   http://localhost:8080/todos -d '{"kind":"work","text":"Learn more Rust"}'
//! ```
//!
//! Fetch the current TODOs:
//!
//! ```shell
//! curl http://localhost:8080/todos
//! ```
//!
//! Add a TODO the `home` policy forbids (HTTP 403):
//!
//! ```shell
//! curl -w "%{http_code}" -XPOST -H "content-type: application/json" -H "x-user: alice" \
//!   http://localhost:8080/todos -d '{"kind":"home","text":"Learn more Rust"}'
//! ```
//!
//! Replace the `home` policy, letting `alice` hack on Rust at home (HTTP 204),
//! then add that TODO again (HTTP 202):
//!
//! ```shell
//! curl -w "%{http_code}" -XPUT http://localhost:8080/policies/home \
//!   -d 'user == "alice" || !todo.text.contains("Rust")'
//! ```
//!
//! A policy that doesn't compile is rejected when it's set (HTTP 400):
//!
//! ```shell
//! curl -XPUT http://localhost:8080/policies/home -d 'user =='
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use cel::common::types::{CelInt, CelString};
use cel::{Context, Env, Program, Value};
use serde::{Deserialize, Serialize};

// The policies the app starts with. Both cap a TODO at 10 words, then...
// Don't watch TV at work!
const WORK_TODO_POLICY: &str = "todo.text.wordCount() <= 10 && !todo.text.contains('TV')";
// Don't hack on Rust at home!
const HOME_TODO_POLICY: &str = "todo.text.wordCount() <= 10 && !todo.text.contains('Rust')";

// TODOs carry some text and have a kind
#[derive(Clone, Deserialize, Serialize)]
struct Todo {
    text: String,
    kind: TodoKind,
}

// TODOs are either for work or for home
#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum TodoKind {
    Work,
    Home,
}

// A function policies can call, as `todo.text.wordCount()`
fn word_count(this: &CelString<'_>) -> CelInt {
    CelInt::from(this.inner().split_whitespace().count() as i64)
}

// The policy of each kind of TODO, and the environment they're compiled and
// executed with. Shared by all requests.
struct Policies {
    env: Arc<Env>,
    programs: RwLock<HashMap<TodoKind, Arc<Program>>>,
}

impl Policies {
    // The standard environment, plus our own functions policies may call:
    // declared once here, rather than on every request's context
    fn new() -> Result<Self, cel::DeclarationError> {
        let mut env = Env::stdlib();
        cel::add_member_overload!(env, fn word_count: (CelString) -> CelInt)?;
        Ok(Self {
            env: Arc::new(env),
            programs: RwLock::default(),
        })
    }

    // Compile a policy once, when it's set, rather than every time a TODO is
    // checked against it. A policy that doesn't compile is rejected here, and
    // the one it would have replaced stays.
    fn set(&self, kind: TodoKind, policy: &str) -> Result<(), cel::ParseErrors> {
        let program = self.env.compile(policy)?;
        self.programs
            .write()
            .unwrap()
            .insert(kind, Arc::new(program));
        Ok(())
    }

    // Determine whether `user` may add a given TODO
    fn allows(&self, user: &str, todo: &Todo) -> Result<bool, TodosError> {
        // Take the policy out of the lock, so it isn't held while executing.
        // A kind of TODO without a policy is allowed nothing.
        let Some(program) = self.programs.read().unwrap().get(&todo.kind).cloned() else {
            return Ok(false);
        };

        // This request's input goes in a context of its own: only the
        // environment is shared with the other requests
        let mut ctx = Context::with_env(Arc::clone(&self.env));
        ctx.add_variable("user", user)?;
        ctx.add_variable("todo", todo)?;

        // Execute the policy: anything but a Boolean is an error
        match program.execute(&ctx)? {
            Value::Bool(allowed) => Ok(allowed),
            _ => Err(TodosError::NotBool),
        }
    }
}

// Custom error type
#[derive(Debug, thiserror::Error)]
enum TodosError {
    #[error("CEL declaration error: {0}")]
    Declaration(#[from] cel::DeclarationError),
    #[error("CEL parse error: {0}")]
    Parse(#[from] cel::ParseErrors),
    #[error("CEL serialization error: {0}")]
    Serialization(#[from] cel::SerializationError),
    #[error("CEL execution error: {0}")]
    Execution(#[from] cel::ExecutionError),
    #[error("policy didn't evaluate to a Boolean")]
    NotBool,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// The state attached to the HTTP router
#[derive(Clone)]
struct AppState {
    todos: Arc<Mutex<Vec<Todo>>>,
    policies: Arc<Policies>,
}

// POST a new TODO to the list, as the user named by the `x-user` header
async fn add_todo(
    State(AppState { todos, policies }): State<AppState>,
    headers: HeaderMap,
    Json(todo): Json<Todo>,
) -> StatusCode {
    let Some(user) = headers.get("x-user").and_then(|user| user.to_str().ok()) else {
        return StatusCode::UNAUTHORIZED;
    };

    match policies.allows(user, &todo) {
        // If allowed, add it to the TODOs and return HTTP 202
        Ok(true) => {
            todos.lock().unwrap().push(todo);
            StatusCode::ACCEPTED
        }
        // If not, return HTTP 403
        Ok(false) => StatusCode::FORBIDDEN,
        // If the policy failed to execute, return HTTP 500
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// GET the current list of TODOs
async fn list_todos(State(AppState { todos, .. }): State<AppState>) -> Json<Vec<Todo>> {
    Json(todos.lock().unwrap().clone())
}

// PUT the policy of a kind of TODO, its CEL source as the body
async fn set_policy(
    State(AppState { policies, .. }): State<AppState>,
    Path(kind): Path<TodoKind>,
    policy: String,
) -> Response {
    match policies.set(kind, &policy) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        // If it doesn't compile, return HTTP 400 with the parse errors
        Err(errors) => (StatusCode::BAD_REQUEST, errors.to_string()).into_response(),
    }
}

#[tokio::main]
async fn main() -> Result<(), TodosError> {
    let policies = Policies::new()?;
    policies.set(TodoKind::Work, WORK_TODO_POLICY)?;
    policies.set(TodoKind::Home, HOME_TODO_POLICY)?;

    let state = AppState {
        todos: Arc::new(Mutex::new(Vec::new())),
        policies: Arc::new(policies),
    };

    let app = Router::new()
        .route("/todos", get(list_todos).post(add_todo))
        .route("/policies/:kind", put(set_policy))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    Ok(axum::serve(listener, app).await?)
}
