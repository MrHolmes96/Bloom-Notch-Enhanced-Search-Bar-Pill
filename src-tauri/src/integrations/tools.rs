//! Finding the right tool on an app's MCP server and filling in its inputs
//! from the tool's own description (its name, its text and its JSON schema),
//! so Bloom keeps working whatever the app calls its tools. The choice can
//! also be made by hand (Settings > Integrations), which always wins.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::mcp::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    CreateTask,
    Lists,
    Tasks,
    CompleteTask,
    /// Moving a task to another column (Today, This week, …).
    MoveTask,
    /// Changing a task's place within its column.
    ReorderTask,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::CreateTask,
        Kind::Lists,
        Kind::Tasks,
        Kind::CompleteTask,
        Kind::MoveTask,
        Kind::ReorderTask,
    ];

    /// The name used for a hand-made choice in the settings.
    pub fn id(self) -> &'static str {
        match self {
            Kind::CreateTask => "create",
            Kind::Lists => "lists",
            Kind::Tasks => "tasks",
            Kind::CompleteTask => "complete",
            Kind::MoveTask => "move",
            Kind::ReorderTask => "reorder",
        }
    }
}

/// A name's words: `createTask`, `create_task` and `create-task` all give
/// `["create", "task"]`.
pub fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            previous_lower = false;
            continue;
        }
        if c.is_uppercase() && previous_lower && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        previous_lower = c.is_lowercase() || c.is_ascii_digit();
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn has(words: &[String], any: &[&str]) -> bool {
    words.iter().any(|w| any.contains(&w.as_str()))
}

const TASK: &[&str] = &["task", "tasks", "todo", "todos", "item", "items"];
const CREATE: &[&str] = &["create", "add", "new", "make", "insert", "upsert", "save"];
const READ: &[&str] = &[
    "get", "list", "search", "find", "fetch", "read", "query", "all", "show", "view", "my",
    "retrieve",
];
const DONE: &[&str] = &[
    "complete",
    "completed",
    "done",
    "finish",
    "check",
    "mark",
    "close",
];
const DESTROY: &[&str] = &["delete", "remove", "archive", "trash"];

fn properties_of(schema: &Value) -> Map<String, Value> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default()
}

fn properties(tool: &Tool) -> Map<String, Value> {
    properties_of(&tool.schema)
}

fn required_of(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn enum_values(property: &Value) -> Vec<String> {
    property
        .get("enum")
        .and_then(|e| e.as_array())
        .map(|e| {
            e.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// An "action" input (`action: create | update | delete`) and its value for
/// one of `verbs`, for tools that do several things.
fn action_value(props: &Map<String, Value>, verbs: &[&str]) -> Option<(String, String)> {
    props.iter().find_map(|(key, property)| {
        let w = words(key);
        if !has(
            &w,
            &["action", "operation", "op", "mode", "command", "method"],
        ) {
            return None;
        }
        enum_values(property)
            .into_iter()
            .find(|v| has(&words(v), verbs))
            .map(|v| (key.clone(), v))
    })
}

const TITLE_KEYS: &[&str] = &[
    "title",
    "name",
    "task_title",
    "taskTitle",
    "task_name",
    "taskName",
    "text",
    "content",
    "summary",
];

fn find_key(props: &Map<String, Value>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| props.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned())
}

fn is_text(property: &Value) -> bool {
    match property.get("type") {
        Some(Value::String(t)) => t == "string",
        Some(Value::Array(types)) => types.iter().any(|t| t == "string"),
        None => true,
        _ => false,
    }
}

fn title_key(schema: &Value) -> Option<String> {
    let props = properties_of(schema);
    find_key(&props, TITLE_KEYS).or_else(|| {
        required_of(schema).into_iter().find(|k| {
            props.get(k).is_some_and(is_text)
                && !has(
                    &words(k),
                    &[
                        "list",
                        "id",
                        "column",
                        "status",
                        "date",
                        "time",
                        "action",
                        "operation",
                    ],
                )
        })
    })
}

/// Where a task goes when it isn't at the top: `task: {title}` or
/// `tasks: [{title}]`. Gives the key, the task's schema, and whether it's a list.
fn nested_task(schema: &Value) -> Option<(String, Value, bool)> {
    properties_of(schema)
        .into_iter()
        .find_map(|(key, property)| {
            if property.get("type").and_then(|t| t.as_str()) == Some("array") {
                let item = property.get("items")?.clone();
                title_key(&item).map(|_| (key, item, true))
            } else if property.get("properties").is_some() {
                title_key(&property).map(|_| (key, property, false))
            } else {
                None
            }
        })
}

fn takes_a_title(tool: &Tool) -> bool {
    title_key(&tool.schema).is_some() || nested_task(&tool.schema).is_some()
}

/// A boolean "done" input, or a status input that has a done value.
fn done_field_of(props: &Map<String, Value>) -> Option<(String, Value)> {
    if let Some(key) = find_key(
        props,
        &[
            "completed",
            "done",
            "is_completed",
            "isCompleted",
            "is_done",
            "isDone",
            "complete",
            "checked",
        ],
    ) {
        return Some((key, Value::Bool(true)));
    }
    let key = find_key(props, &["status", "state"])?;
    let value = enum_values(&props[&key]).into_iter().find(|v| {
        ["done", "completed", "complete", "finished"].contains(&v.to_lowercase().as_str())
    })?;
    Some((key, Value::String(value)))
}

fn score(tool: &Tool, kind: Kind) -> i32 {
    let name = words(&tool.name);
    let text = words(&tool.description);
    let props = properties(tool);
    let joined = name.join("_");
    let task_name = has(&name, TASK);
    let task_text = has(&text, &["task", "tasks", "todo", "todos"]);
    let destroy = has(&name, DESTROY);
    let edits = has(
        &name,
        &["update", "edit", "modify", "change", "move", "rename"],
    );
    match kind {
        Kind::CreateTask => {
            if destroy || has(&name, &["subtask", "subtasks", "note", "notes", "comment"]) {
                return 0;
            }
            let mut score = if task_name && has(&name, CREATE) {
                10
            } else if task_name && action_value(&props, CREATE).is_some() {
                8
            } else if (task_name || task_text) && has(&text, &["create", "creates", "add", "adds"])
            {
                6
            } else {
                0
            };
            if score == 0 || !takes_a_title(tool) {
                return 0;
            }
            if edits {
                score -= 2;
            }
            if joined == "create_task" || joined == "add_task" {
                score += 5;
            }
            score
        }
        Kind::Lists => {
            if task_name || destroy || edits || has(&name, CREATE) || has(&name, &["subtask"]) {
                return 0;
            }
            if has(&name, &["lists"])
                || (has(&name, &["list"]) && has(&name, READ) && name.len() >= 2)
            {
                10 + if joined == "get_lists" || joined == "list_lists" {
                    3
                } else {
                    0
                }
            } else if has(&text, &["lists"])
                && has(&text, &["get", "gets", "returns", "retrieve", "all"])
            {
                5
            } else {
                0
            }
        }
        Kind::Tasks => {
            if destroy
                || edits
                || has(&name, CREATE)
                || has(&name, DONE)
                || has(&name, &["subtask", "schedule"])
            {
                return 0;
            }
            if task_name && has(&name, READ) {
                let mut score = 10 + if has(&name, &["tasks"]) { 2 } else { 0 };
                // All of a list's tasks, not a slice of them (today's, this
                // week's, the scheduled ones, the ones matching a search).
                if joined == "list_tasks" || joined == "get_tasks" || joined == "get_all_tasks" {
                    score += 4;
                }
                if list_key(&props).is_some() {
                    score += 2;
                }
                if has(
                    &name,
                    &[
                        "today",
                        "todays",
                        "week",
                        "weeks",
                        "scheduled",
                        "deleted",
                        "search",
                        "counts",
                        "count",
                    ],
                ) {
                    score -= 4;
                }
                if !has(&name, &["tasks"])
                    && required_of(&tool.schema)
                        .iter()
                        .any(|r| has(&words(r), &["id"]))
                {
                    // One task by its id.
                    score -= 6;
                }
                score.max(1)
            } else if task_text
                && has(
                    &text,
                    &["get", "gets", "returns", "retrieve", "search", "find"],
                )
                && !has(&text, &["create", "creates"])
            {
                5
            } else {
                0
            }
        }
        Kind::ReorderTask => {
            if !task_name || destroy || has(&name, CREATE) || has(&name, &["subtask", "subtasks"]) {
                return 0;
            }
            let places = props.keys().any(|k| {
                has(
                    &words(k),
                    &["before", "after", "position", "index", "order", "rank"],
                )
            });
            if has(&name, &["reorder", "sort", "rank", "position"]) && places {
                10
            } else {
                0
            }
        }
        Kind::MoveTask => {
            if !task_name
                || destroy
                || has(&name, CREATE)
                || has(&name, &["subtask", "subtasks", "note", "notes", "comment"])
                || move_target(&tool.schema).is_none()
            {
                return 0;
            }
            if has(&name, &["move"]) {
                10
            } else if edits || has(&name, &["schedule", "reschedule", "set"]) {
                6
            } else {
                0
            }
        }
        Kind::CompleteTask => {
            if !task_name && !task_text {
                return 0;
            }
            if destroy
                || has(
                    &name,
                    &["subtask", "uncomplete", "incomplete", "undo", "reopen"],
                )
                // Reading the done ones isn't ticking one off.
                || has(&name, &["get", "list", "search", "fetch", "find", "read", "count"])
            {
                return 0;
            }
            if task_name && has(&name, DONE) {
                return 10;
            }
            if task_name && action_value(&props, DONE).is_some() {
                return 8;
            }
            if task_name && edits && done_field_of(&props).is_some() {
                return 5;
            }
            0
        }
    }
}

/// The tool for `kind`: the one chosen by hand when it still exists, else
/// the best match.
pub fn pick<'a>(tools: &'a [Tool], kind: Kind, chosen: Option<&str>) -> Option<&'a Tool> {
    if let Some(name) = chosen.filter(|n| !n.is_empty()) {
        if let Some(tool) = tools.iter().find(|t| t.name == name) {
            return Some(tool);
        }
    }
    tools
        .iter()
        .map(|t| (score(t, kind), t))
        .filter(|(s, _)| *s > 0)
        .max_by_key(|(s, _)| *s)
        .map(|(_, t)| t)
}

fn list_key(props: &Map<String, Value>) -> Option<String> {
    let mut keys: Vec<&String> = props
        .keys()
        .filter(|k| {
            let w = words(k);
            has(&w, &["list", "listid", "project"]) && !has(&w, &["column", "lists"])
        })
        .collect();
    // An id is the safest thing to send.
    keys.sort_by_key(|k| !has(&words(k), &["id"]));
    keys.first().map(|k| k.to_string())
}

fn column_key(props: &Map<String, Value>) -> Option<String> {
    props
        .keys()
        .find(|k| {
            has(
                &words(k),
                &["column", "section", "bucket", "stage", "board", "status"],
            )
        })
        .cloned()
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A list to put a task in or read tasks from.
#[derive(Clone, Debug)]
pub struct ListRef {
    pub id: String,
    pub name: String,
}

fn put_list(
    args: &mut Map<String, Value>,
    props: &Map<String, Value>,
    list: Option<&ListRef>,
) -> bool {
    if let (Some(key), Some(list)) = (list_key(props), list) {
        let by_name = has(&words(&key), &["name", "title"]);
        let value = if by_name { &list.name } else { &list.id };
        args.insert(key, Value::String(value.clone()));
        return true;
    }
    false
}

fn put_column(args: &mut Map<String, Value>, props: &Map<String, Value>, column: Option<&str>) {
    let (Some(key), Some(column)) = (column_key(props), column) else {
        return;
    };
    let property = &props[&key];
    let values = enum_values(property);
    let wanted = squash(column);
    if let Some(value) = values.iter().find(|v| squash(v) == wanted) {
        args.insert(key, Value::String(value.clone()));
    } else if values.is_empty()
        && property
            .get("description")
            .and_then(|d| d.as_str())
            .is_some_and(|d| d.to_lowercase().contains(&column.to_lowercase()))
    {
        args.insert(key, Value::String(column.to_string()));
    }
}

fn check_required(schema: &Value, args: &Map<String, Value>) -> Result<(), String> {
    for name in required_of(schema) {
        if !args.contains_key(&name) {
            let w = words(&name);
            return Err(if has(&w, &["list", "listid", "project", "board"]) {
                "Pick a Blitzit list first".to_string()
            } else {
                format!(
                    "Blitzit wants \"{}\" for this, which Bloom can't fill in",
                    name
                )
            });
        }
    }
    Ok(())
}

/// One task's own fields: title, list, column.
fn task_object(
    schema: &Value,
    title: &str,
    list: Option<&ListRef>,
    column: Option<&str>,
) -> Result<Map<String, Value>, String> {
    let props = properties_of(schema);
    let mut task = Map::new();
    let key = title_key(schema).ok_or("Blitzit's task tool takes no title")?;
    task.insert(key, Value::String(title.to_string()));
    put_list(&mut task, &props, list);
    put_column(&mut task, &props, column);
    check_required(schema, &task)?;
    Ok(task)
}

/// The inputs for creating a task called `title`.
pub fn create_args(
    tool: &Tool,
    title: &str,
    list: Option<&ListRef>,
    column: Option<&str>,
) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    if let Some((key, value)) = action_value(&props, CREATE) {
        args.insert(key, Value::String(value));
    }
    if let Some(key) = title_key(&tool.schema) {
        args.insert(key, Value::String(title.to_string()));
        put_list(&mut args, &props, list);
        put_column(&mut args, &props, column);
    } else if let Some((key, schema, many)) = nested_task(&tool.schema) {
        let task = task_object(&schema, title, list, column)?;
        args.insert(
            key,
            if many {
                Value::Array(vec![Value::Object(task)])
            } else {
                Value::Object(task)
            },
        );
        // A list given beside the task rather than in it.
        put_list(&mut args, &props, list);
    } else {
        return Err("Blitzit's task tool takes no title".into());
    }
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
}

/// The columns a new task can go in (Today, This week, …), when the tool says.
pub fn column_choices(tool: &Tool) -> Vec<String> {
    let schema = if title_key(&tool.schema).is_some() {
        tool.schema.clone()
    } else {
        nested_task(&tool.schema)
            .map(|(_, s, _)| s)
            .unwrap_or(Value::Null)
    };
    let props = properties_of(&schema);
    column_key(&props)
        .map(|key| enum_values(&props[&key]))
        .unwrap_or_default()
        .into_iter()
        .filter(|v| !["done", "completed", "archived"].contains(&v.to_lowercase().as_str()))
        .collect()
}

/// The inputs for reading tasks, from one list when the tool takes one.
pub fn tasks_args(tool: &Tool, list: Option<&ListRef>) -> Result<Value, String> {
    tasks_page_args(tool, list, None)
}

/// The inputs for reading tasks, done ones too (the board's Done column),
/// as many at once as the tool allows, from `cursor` on when paging.
pub fn tasks_page_args(
    tool: &Tool,
    list: Option<&ListRef>,
    cursor: Option<&str>,
) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    if let Some((key, value)) = action_value(&props, READ) {
        args.insert(key, Value::String(value));
    }
    put_list(&mut args, &props, list);
    if let Some(key) = find_key(
        &props,
        &[
            "includeCompleted",
            "include_completed",
            "includeDone",
            "include_done",
            "showCompleted",
        ],
    ) {
        args.insert(key, Value::Bool(true));
    }
    if let Some(key) = find_key(&props, &["limit", "pageSize", "page_size", "per_page"]) {
        args.insert(key, Value::from(100));
    }
    if let (Some(cursor), Some(key)) = (
        cursor,
        find_key(&props, &["cursor", "nextCursor", "pageToken", "page_token"]),
    ) {
        args.insert(key, Value::String(cursor.to_string()));
    }
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
}

/// The cursor for the next page of an answer, when there is one.
pub fn next_cursor(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    [
        "nextCursor",
        "next_cursor",
        "cursor",
        "nextPageToken",
        "next_page_token",
    ]
    .iter()
    .find_map(|k| object.get(*k))
    .and_then(|v| v.as_str())
    .filter(|c| !c.is_empty())
    .map(str::to_string)
}

/// The inputs for reading the lists.
pub fn lists_args(tool: &Tool) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    if let Some((key, value)) = action_value(&props, READ) {
        args.insert(key, Value::String(value));
    }
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
}

/// The inputs for marking task `id` done.
pub fn complete_args(tool: &Tool, id: &str, list: Option<&ListRef>) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    if !put_task_id(&mut args, &props, id) {
        return Err("Blitzit's tool doesn't say which task".into());
    }
    put_list(&mut args, &props, list);
    if let Some((key, value)) = action_value(&props, DONE) {
        args.insert(key, Value::String(value));
    } else if score(tool, Kind::CompleteTask) < 10 {
        let (key, value) =
            done_field_of(&props).ok_or("Blitzit can't mark tasks done from here")?;
        args.insert(key, value);
    } else if let Some((key, value)) = done_field_of(&props) {
        if required_of(&tool.schema).contains(&key) {
            args.insert(key, value);
        }
    }
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
}

/// The task an action is for: `taskId: "T1"`, or `taskIds: ["T1"]` for an
/// action that takes several.
fn put_task_id(args: &mut Map<String, Value>, props: &Map<String, Value>, id: &str) -> bool {
    if let Some(key) = find_key(
        props,
        &[
            "task_id", "taskId", "id", "task", "taskID", "item_id", "itemId",
        ],
    ) {
        args.insert(key, Value::String(id.to_string()));
        return true;
    }
    if let Some(key) = find_key(
        props,
        &["task_ids", "taskIds", "ids", "tasks", "item_ids", "itemIds"],
    ) {
        if props[&key].get("type").and_then(|t| t.as_str()) == Some("array") {
            args.insert(key, Value::Array(vec![Value::String(id.to_string())]));
            return true;
        }
    }
    false
}

/// The inputs for putting task `id` just before `before` (or after `after`,
/// or at the top when neither is given) in its column.
pub fn reorder_args(
    tool: &Tool,
    id: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    if !put_task_id(&mut args, &props, id) {
        return Err("Blitzit's reorder action doesn't say which task".into());
    }
    let key_with = |word: &str| {
        props
            .keys()
            .find(|k| {
                let w = words(k);
                has(&w, &[word]) && !w.iter().any(|x| x == "position")
            })
            .cloned()
    };
    let placed = match (before, after) {
        (Some(before), _) if key_with("before").is_some() => {
            args.insert(
                key_with("before").unwrap_or_default(),
                Value::String(before.into()),
            );
            true
        }
        (_, Some(after)) if key_with("after").is_some() => {
            args.insert(
                key_with("after").unwrap_or_default(),
                Value::String(after.into()),
            );
            true
        }
        _ => false,
    };
    if !placed {
        let key = find_key(&props, &["position"]).ok_or("Blitzit can't place the task there")?;
        let values = enum_values(&props[&key]);
        let wanted = if after.is_some() { "bottom" } else { "top" };
        let value = values
            .iter()
            .find(|v| v.eq_ignore_ascii_case(wanted))
            .cloned()
            .ok_or("Blitzit can't place the task there")?;
        args.insert(key, Value::String(value));
    }
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
}

/// Where a move tool takes the column: at the top (`None`), or inside an
/// object input (`updates: {column}`), by that input's key.
fn move_target(schema: &Value) -> Option<Option<String>> {
    let props = properties_of(schema);
    if column_key(&props).is_some() {
        return Some(None);
    }
    props.iter().find_map(|(key, property)| {
        let inner = properties_of(property);
        column_key(&inner).map(|_| Some(key.clone()))
    })
}

/// The inputs for moving task `id` to `column`. An error when the tool
/// can't take that column, so a move is never reported done when it wasn't.
pub fn move_args(
    tool: &Tool,
    id: &str,
    list: Option<&ListRef>,
    column: &str,
) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    if !put_task_id(&mut args, &props, id) {
        return Err("Blitzit's move action doesn't say which task".into());
    }
    put_list(&mut args, &props, list);
    if let Some((key, value)) = action_value(&props, &["move", "update", "edit", "change", "set"]) {
        args.insert(key, Value::String(value));
    }
    let not_taken = || {
        format!(
            "Blitzit's move action doesn't take the column \"{}\"",
            column
        )
    };
    match move_target(&tool.schema) {
        Some(None) => {
            put_column(&mut args, &props, Some(column));
            let key = column_key(&props).unwrap_or_default();
            if !args.contains_key(&key) {
                // No list of values to check against: send it as Blitzit names it.
                if enum_values(&props[&key]).is_empty() {
                    args.insert(key, Value::String(column.to_string()));
                } else {
                    return Err(not_taken());
                }
            }
        }
        Some(Some(outer)) => {
            let inner_schema = props[&outer].clone();
            let inner = properties_of(&inner_schema);
            let mut nested = Map::new();
            put_column(&mut nested, &inner, Some(column));
            let key = column_key(&inner).unwrap_or_default();
            if !nested.contains_key(&key) {
                if enum_values(&inner[&key]).is_empty() {
                    nested.insert(key, Value::String(column.to_string()));
                } else {
                    return Err(not_taken());
                }
            }
            args.insert(outer, Value::Object(nested));
        }
        None => return Err("Blitzit's action can't move tasks between columns".into()),
    }
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
}

// ---------------------------------------------------------------- reading answers

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub done: bool,
    /// Today, This week, …; empty when the answer doesn't say.
    pub column: String,
    /// The list it belongs to, when the answer says.
    pub list: String,
    /// Its colour, when the answer says (a list's, in Blitzit).
    #[serde(default)]
    pub color: String,
    /// Its emoji, when it has one instead of a colour (a list's, in Blitzit).
    #[serde(default)]
    pub emoji: String,
    /// Its place in its column, when the answer says (sorts as text or as a
    /// number, whichever it is).
    #[serde(default)]
    pub order: String,
}

const ID_KEYS: &[&str] = &[
    "id", "_id", "uuid", "task_id", "taskId", "list_id", "listId",
];
const NAME_KEYS: &[&str] = &[
    "title",
    "name",
    "text",
    "content",
    "task_title",
    "list_name",
];

fn string_field(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .and_then(|(_, v)| match v {
                Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                // `column: {"name": "Today"}`
                Value::Object(inner) => string_field(inner, &["name", "title"]),
                _ => None,
            })
    })
}

fn as_item(object: &Map<String, Value>, parent_key: &str) -> Option<Item> {
    let id = string_field(object, ID_KEYS)?;
    let name = string_field(object, NAME_KEYS)?;
    let flag = |keys: &[&str]| {
        keys.iter().any(|key| {
            object
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(key) && v.as_bool() == Some(true))
        })
    };
    let status = string_field(object, &["status", "state"]).unwrap_or_default();
    let done = flag(&[
        "completed",
        "done",
        "is_completed",
        "isCompleted",
        "is_done",
        "isDone",
        "checked",
    ]) || ["done", "completed", "complete", "finished"]
        .contains(&status.to_lowercase().as_str())
        || object.iter().any(|(k, v)| {
            (k.eq_ignore_ascii_case("completed_at") || k.eq_ignore_ascii_case("completedAt"))
                && !v.is_null()
        });
    let column = string_field(
        object,
        &[
            "column",
            "column_name",
            "columnName",
            "board",
            "section",
            "bucket",
            "when",
        ],
    )
    .or_else(|| (!status.is_empty() && !done).then(|| status.clone()))
    .unwrap_or_else(|| parent_key.to_string());
    // On the Done board means done.
    let done =
        done || ["done", "completed", "complete", "finished"].contains(&squash(&column).as_str());
    let order = string_field(
        object,
        &[
            "position",
            "order",
            "rank",
            "sortOrder",
            "sort_order",
            "sortIndex",
            "index",
        ],
    )
    .unwrap_or_default();
    // A list's mark: `avatar: {type: "color" | "emoji", value}`.
    let avatar = object
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("avatar"))
        .and_then(|(_, v)| v.as_object());
    let avatar_of = |kind: &str| {
        avatar
            .filter(|a| a.get("type").and_then(|t| t.as_str()) == Some(kind))
            .and_then(|a| a.get("value").and_then(|v| v.as_str()))
            .map(str::to_string)
    };
    let emoji = avatar_of("emoji").unwrap_or_default();
    let list = string_field(
        object,
        &["list_name", "listName", "list", "list_id", "listId"],
    )
    .unwrap_or_default();
    let color = avatar_of("color")
        .or_else(|| string_field(object, &["color", "colour", "hex", "color_hex", "colorHex"]))
        .filter(|c| c.len() <= 32)
        .unwrap_or_default();
    Some(Item {
        id,
        name,
        done,
        column,
        list,
        color,
        emoji,
        order,
    })
}

/// The items in an answer: the objects with an id and a name in its
/// shallowest lists (a task's own sub-tasks deeper down are left out). Lists
/// grouped under a key (`"today": [...]`) give that key as their column.
pub fn items(value: &Value) -> Vec<Item> {
    let mut level: Vec<(&Value, String)> = vec![(value, String::new())];
    for _ in 0..6 {
        let mut found = Vec::new();
        let mut next = Vec::new();
        for (node, key) in &level {
            match node {
                Value::Array(list) => {
                    let here: Vec<Item> = list
                        .iter()
                        .filter_map(|v| v.as_object().and_then(|o| as_item(o, key)))
                        .collect();
                    if here.is_empty() {
                        next.extend(list.iter().map(|v| (v, key.clone())));
                    } else {
                        found.extend(here);
                    }
                }
                Value::Object(object) => {
                    for (k, v) in object {
                        next.push((v, k.clone()));
                    }
                }
                _ => {}
            }
        }
        if !found.is_empty() {
            return found;
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    Vec::new()
}

/// The task an add answered with, when it says: the one called `title` in
/// a list, or the task object itself (`{id, title}` or `{task: {id, title}}`).
pub fn created_item(value: &Value, title: &str) -> Option<Item> {
    let wanted = squash(title);
    if let Some(found) = items(value).into_iter().find(|i| squash(&i.name) == wanted) {
        return Some(found);
    }
    let object = value.as_object()?;
    if let Some(item) = as_item(object, "") {
        return Some(item);
    }
    object
        .values()
        .filter_map(|v| v.as_object())
        .find_map(|inner| as_item(inner, ""))
}

/// Whether two task titles are the same task's, spacing and case aside.
pub fn same_title(a: &str, b: &str) -> bool {
    squash(a) == squash(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, schema: Value) -> Tool {
        Tool {
            name: name.into(),
            description: String::new(),
            schema,
        }
    }

    fn described(name: &str, description: &str, schema: Value) -> Tool {
        Tool {
            name: name.into(),
            description: description.into(),
            schema,
        }
    }

    /// Blitzit's own actions, as its connection lists them (Blitzit 3.0).
    fn blitzit_tools() -> Vec<Tool> {
        let raw: Vec<Value> =
            serde_json::from_str(include_str!("testdata/blitzit-tools.json")).unwrap();
        raw.into_iter()
            .map(|t| Tool {
                name: t["name"].as_str().unwrap().into(),
                description: t["description"].as_str().unwrap_or_default().into(),
                schema: t["inputSchema"].clone(),
            })
            .collect()
    }

    #[test]
    fn blitzits_own_actions_are_picked_and_filled_in() {
        let tools = blitzit_tools();
        let name = |kind| pick(&tools, kind, None).map(|t| t.name.clone());
        assert_eq!(name(Kind::CreateTask).as_deref(), Some("create_task"));
        assert_eq!(name(Kind::Lists).as_deref(), Some("list_lists"));
        assert_eq!(name(Kind::Tasks).as_deref(), Some("list_tasks"));
        assert_eq!(name(Kind::CompleteTask).as_deref(), Some("complete_task"));
        assert_eq!(name(Kind::MoveTask).as_deref(), Some("move_tasks_to_board"));
        assert_eq!(name(Kind::ReorderTask).as_deref(), Some("reorder_task"));

        let list = ListRef {
            id: "L1".into(),
            name: "test list".into(),
        };
        let create = pick(&tools, Kind::CreateTask, None).unwrap();
        assert_eq!(
            column_choices(create),
            vec!["backlog".to_string(), "today".into(), "this-week".into()]
        );
        assert_eq!(
            create_args(create, "Gym", Some(&list), Some("this-week")).unwrap(),
            json!({"title": "Gym", "listId": "L1", "board": "this-week"})
        );
        let read = pick(&tools, Kind::Tasks, None).unwrap();
        assert_eq!(
            tasks_page_args(read, Some(&list), Some("c2")).unwrap(),
            json!({"listId": "L1", "includeCompleted": true, "limit": 100, "cursor": "c2"})
        );
        let mover = pick(&tools, Kind::MoveTask, None).unwrap();
        assert_eq!(
            move_args(mover, "T1", Some(&list), "This Week").unwrap(),
            json!({"taskIds": ["T1"], "board": "this-week"})
        );
        // Done is a board too: ticking off needs no revision that way.
        assert_eq!(
            move_args(mover, "T1", None, "done").unwrap(),
            json!({"taskIds": ["T1"], "board": "done"})
        );
        let reorder = pick(&tools, Kind::ReorderTask, None).unwrap();
        assert_eq!(
            reorder_args(reorder, "T1", Some("T2"), None).unwrap(),
            json!({"taskId": "T1", "beforeTaskId": "T2"})
        );
        assert_eq!(
            reorder_args(reorder, "T1", None, Some("T3")).unwrap(),
            json!({"taskId": "T1", "afterTaskId": "T3"})
        );
        assert_eq!(
            reorder_args(reorder, "T1", None, None).unwrap(),
            json!({"taskId": "T1", "position": "top"})
        );
    }

    #[test]
    fn blitzit_tasks_and_lists_read_right() {
        let answer = json!({"tasks": [
            {"id": "T1", "title": "Gym", "board": "today", "position": "a2"},
            {"id": "T2", "title": "Done one", "board": "done", "position": "a1"}
        ], "nextCursor": "c2"});
        let found = items(&answer);
        assert_eq!(found[0].column, "today");
        assert!(!found[0].done);
        assert!(found[1].done);
        assert_eq!(found[0].order, "a2");
        assert_eq!(next_cursor(&answer).as_deref(), Some("c2"));
        let lists = items(&json!({"lists": [
            {"id": "L1", "name": "test list", "avatar": {"type": "color", "value": "#22c55e"}},
            {"id": "L2", "name": "Work", "avatar": {"type": "emoji", "value": "💼"}}
        ]}));
        assert_eq!(lists[0].color, "#22c55e");
        assert_eq!(lists[1].emoji, "💼");
    }

    #[test]
    fn moves_go_to_the_column_or_fail_honestly() {
        let tools = vec![
            tool(
                "update_task",
                json!({"properties": {"task_id": {}, "completed": {"type": "boolean"}}}),
            ),
            tool(
                "move_task",
                json!({"properties": {"taskId": {"type": "string"}, "listId": {"type": "string"}, "column": {"enum": ["BACKLOG", "THIS_WEEK", "TODAY"]}}, "required": ["taskId", "column"]}),
            ),
        ];
        // update_task has no column, so it can't move tasks.
        let mover = pick(&tools, Kind::MoveTask, None).unwrap();
        assert_eq!(mover.name, "move_task");
        let list = ListRef {
            id: "L1".into(),
            name: "Work".into(),
        };
        assert_eq!(
            move_args(mover, "T1", Some(&list), "This week").unwrap(),
            json!({"taskId": "T1", "listId": "L1", "column": "THIS_WEEK"})
        );
        assert!(move_args(mover, "T1", Some(&list), "Someday").is_err());

        let nested = tool(
            "edit_task",
            json!({"properties": {"id": {}, "updates": {"properties": {"section": {"type": "string"}}}}}),
        );
        assert_eq!(
            pick(std::slice::from_ref(&nested), Kind::MoveTask, None)
                .unwrap()
                .name,
            "edit_task"
        );
        assert_eq!(
            move_args(&nested, "T2", None, "today").unwrap(),
            json!({"id": "T2", "updates": {"section": "today"}})
        );
    }

    #[test]
    fn names_split_into_words() {
        assert_eq!(words("createTask"), vec!["create", "task"]);
        assert_eq!(words("get_all-lists"), vec!["get", "all", "lists"]);
        assert_eq!(words("blitzit.add_task"), vec!["blitzit", "add", "task"]);
    }

    #[test]
    fn the_right_tools_are_picked() {
        let title = json!({"properties": {"title": {"type": "string"}}});
        let tools = vec![
            tool("create_list", title.clone()),
            tool("get_lists", json!({})),
            tool("list_tasks", json!({})),
            tool("create_task", title.clone()),
            tool("create_subtask", title.clone()),
            tool(
                "update_task",
                json!({"properties": {"task_id": {}, "completed": {"type": "boolean"}}}),
            ),
            tool("delete_task", json!({})),
        ];
        assert_eq!(
            pick(&tools, Kind::CreateTask, None).unwrap().name,
            "create_task"
        );
        assert_eq!(pick(&tools, Kind::Lists, None).unwrap().name, "get_lists");
        assert_eq!(pick(&tools, Kind::Tasks, None).unwrap().name, "list_tasks");
        assert_eq!(
            pick(&tools, Kind::CompleteTask, None).unwrap().name,
            "update_task"
        );
        // A choice made by hand wins.
        assert_eq!(
            pick(&tools, Kind::Tasks, Some("get_lists")).unwrap().name,
            "get_lists"
        );
    }

    #[test]
    fn odd_names_are_found_from_their_text_and_inputs() {
        let tools = vec![
            described(
                "upsertTasks",
                "Create or update tasks in a list",
                json!({"properties": {"tasks": {"type": "array", "items": {"properties": {"title": {"type": "string"}, "listId": {"type": "string"}}, "required": ["title"]}}}, "required": ["tasks"]}),
            ),
            described(
                "manage_task",
                "Create, update, complete or delete a task",
                json!({"properties": {"action": {"enum": ["create", "update", "complete", "delete"]}, "title": {"type": "string"}, "taskId": {"type": "string"}}, "required": ["action"]}),
            ),
        ];
        let create = pick(&tools, Kind::CreateTask, None).unwrap();
        let list = ListRef {
            id: "L1".into(),
            name: "Work".into(),
        };
        let args = create_args(create, "Call mum", Some(&list), None).unwrap();
        if create.name == "manage_task" {
            assert_eq!(args, json!({"action": "create", "title": "Call mum"}));
        } else {
            assert_eq!(
                args,
                json!({"tasks": [{"title": "Call mum", "listId": "L1"}]})
            );
        }
        let complete = pick(&tools, Kind::CompleteTask, None).unwrap();
        assert_eq!(complete.name, "manage_task");
        assert_eq!(
            complete_args(complete, "T1", None).unwrap(),
            json!({"action": "complete", "taskId": "T1"})
        );
    }

    #[test]
    fn task_inputs_follow_the_schema() {
        let create = tool(
            "createTask",
            json!({
                "properties": {
                    "title": {"type": "string"},
                    "listId": {"type": "string"},
                    "column": {"type": "string", "enum": ["BACKLOG", "THIS_WEEK", "TODAY"]}
                },
                "required": ["title", "listId"]
            }),
        );
        let list = ListRef {
            id: "L1".into(),
            name: "Work".into(),
        };
        let args = create_args(&create, "Call mum", Some(&list), Some("Today")).unwrap();
        assert_eq!(
            args,
            json!({"title": "Call mum", "listId": "L1", "column": "TODAY"})
        );
        assert_eq!(
            create_args(&create, "x", None, None).unwrap_err(),
            "Pick a Blitzit list first"
        );
        assert_eq!(
            column_choices(&create),
            vec!["BACKLOG", "THIS_WEEK", "TODAY"]
        );

        let update = tool(
            "update_task",
            json!({"properties": {"task_id": {}, "status": {"enum": ["todo", "done"]}}, "required": ["task_id"]}),
        );
        assert_eq!(
            complete_args(&update, "T9", None).unwrap(),
            json!({"task_id": "T9", "status": "done"})
        );
    }

    #[test]
    fn answers_become_items() {
        let answer = json!({"lists": [{"id": "a", "name": "Work"}, {"id": "b", "name": "Home"}]});
        let found = items(&answer);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].name, "Home");

        let grouped = json!({"today": [{"id": 1, "title": "Gym", "subtasks": [{"id": 2, "title": "x"}]}],
                             "done": [{"id": 3, "title": "Mail", "completed": true}]});
        let found = items(&grouped);
        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .any(|i| i.name == "Gym" && i.column == "today" && !i.done));
        assert!(found.iter().any(|i| i.name == "Mail" && i.done));
    }
}
