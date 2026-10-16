//! Finding the right tool on an app's MCP server and filling in its inputs
//! from the tool's own description (its JSON schema), so Bloom keeps working
//! when the app renames a tool or adds an option.

use serde::Serialize;
use serde_json::{Map, Value};

use super::mcp::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    CreateTask,
    Lists,
    Tasks,
    CompleteTask,
}

/// A tool name's words: `createTask`, `create_task` and `create-task` all give
/// `["create", "task"]`.
pub fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' || c == '.' || c == '/' {
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

fn score(tool: &Tool, kind: Kind) -> i32 {
    let w = words(&tool.name);
    let joined = w.join("_");
    let task = has(&w, &["task", "tasks", "todo", "todos"]);
    let edits = has(
        &w,
        &[
            "update", "edit", "delete", "remove", "move", "rename", "archive", "reorder",
        ],
    );
    match kind {
        Kind::CreateTask => {
            if !task || !has(&w, &["create", "add", "new", "make"]) || edits {
                return 0;
            }
            if has(&w, &["subtask", "subtasks", "note", "notes", "comment"]) {
                return 0;
            }
            10 + if joined == "create_task" || joined == "add_task" {
                5
            } else {
                0
            }
        }
        Kind::Lists => {
            if task || has(&w, &["subtask", "create", "add", "new"]) || edits {
                return 0;
            }
            let lists = has(&w, &["lists"])
                || (has(&w, &["list"])
                    && has(
                        &w,
                        &[
                            "get", "fetch", "all", "my", "read", "search", "find", "list",
                        ],
                    )
                    && w.len() >= 2);
            if !lists {
                return 0;
            }
            10 + if joined == "get_lists" || joined == "list_lists" {
                3
            } else {
                0
            }
        }
        Kind::Tasks => {
            if !task
                || !has(
                    &w,
                    &[
                        "get", "list", "search", "find", "fetch", "read", "query", "all",
                    ],
                )
                || edits
                || has(
                    &w,
                    &["create", "add", "new", "complete", "subtask", "schedule"],
                )
            {
                return 0;
            }
            10 + if has(&w, &["tasks"]) { 2 } else { 0 }
        }
        Kind::CompleteTask => {
            if !task
                || has(
                    &w,
                    &["subtask", "uncomplete", "incomplete", "undo", "delete"],
                )
            {
                return 0;
            }
            if has(
                &w,
                &["complete", "completed", "done", "finish", "check", "mark"],
            ) {
                return 10;
            }
            // An "update task" that can set it done.
            if has(&w, &["update", "edit"]) && done_field(tool).is_some() {
                return 5;
            }
            0
        }
    }
}

pub fn pick(tools: &[Tool], kind: Kind) -> Option<&Tool> {
    tools
        .iter()
        .map(|t| (score(t, kind), t))
        .filter(|(s, _)| *s > 0)
        .max_by_key(|(s, _)| *s)
        .map(|(_, t)| t)
}

fn properties(tool: &Tool) -> Map<String, Value> {
    tool.schema
        .get("properties")
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default()
}

fn required(tool: &Tool) -> Vec<String> {
    tool.schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn is_text(property: &Value) -> bool {
    match property.get("type") {
        Some(Value::String(t)) => t == "string",
        Some(Value::Array(types)) => types.iter().any(|t| t == "string"),
        None => true,
        _ => false,
    }
}

fn find_key(props: &Map<String, Value>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| props.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned())
}

fn list_key(props: &Map<String, Value>) -> Option<String> {
    let mut keys: Vec<&String> = props
        .keys()
        .filter(|k| {
            let w = words(k);
            has(&w, &["list", "listid"]) && !has(&w, &["column", "lists"])
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
                &["column", "section", "bucket", "stage", "status"],
            )
        })
        .cloned()
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

fn put_list(args: &mut Map<String, Value>, props: &Map<String, Value>, list: Option<&ListRef>) {
    if let (Some(key), Some(list)) = (list_key(props), list) {
        let by_name = has(&words(&key), &["name", "title"]);
        let value = if by_name { &list.name } else { &list.id };
        args.insert(key, Value::String(value.clone()));
    }
}

fn check_required(tool: &Tool, args: &Map<String, Value>) -> Result<(), String> {
    for name in required(tool) {
        if !args.contains_key(&name) {
            let w = words(&name);
            return Err(if has(&w, &["list", "listid"]) {
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

/// The inputs for creating a task called `title`.
pub fn create_args(
    tool: &Tool,
    title: &str,
    list: Option<&ListRef>,
    column: Option<&str>,
) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    let title_key = find_key(
        &props,
        &[
            "title",
            "name",
            "task_title",
            "taskTitle",
            "task_name",
            "taskName",
            "text",
            "content",
            "summary",
        ],
    )
    .or_else(|| {
        required(tool).into_iter().find(|k| {
            props.get(k).is_some_and(is_text)
                && !has(
                    &words(k),
                    &["list", "id", "column", "status", "date", "time"],
                )
        })
    })
    .ok_or("Blitzit's task tool takes no title")?;
    args.insert(title_key, Value::String(title.to_string()));
    put_list(&mut args, &props, list);
    if let (Some(key), Some(column)) = (column_key(&props), column) {
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
    check_required(tool, &args)?;
    Ok(Value::Object(args))
}

/// The columns a new task can go in (Today, This week, …), when the tool says.
pub fn column_choices(tool: &Tool) -> Vec<String> {
    let props = properties(tool);
    column_key(&props)
        .map(|key| enum_values(&props[&key]))
        .unwrap_or_default()
        .into_iter()
        .filter(|v| !["done", "completed", "archived"].contains(&v.to_lowercase().as_str()))
        .collect()
}

/// The inputs for reading tasks, from one list when the tool takes one.
pub fn tasks_args(tool: &Tool, list: Option<&ListRef>) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    put_list(&mut args, &props, list);
    check_required(tool, &args)?;
    Ok(Value::Object(args))
}

/// The inputs for reading the lists.
pub fn lists_args(tool: &Tool) -> Result<Value, String> {
    let args = Map::new();
    check_required(tool, &args)?;
    Ok(Value::Object(args))
}

/// A boolean "done" input, or a status input that has a done value.
fn done_field(tool: &Tool) -> Option<(String, Value)> {
    let props = properties(tool);
    if let Some(key) = find_key(
        &props,
        &[
            "completed",
            "done",
            "is_completed",
            "isCompleted",
            "is_done",
            "isDone",
            "complete",
        ],
    ) {
        return Some((key, Value::Bool(true)));
    }
    let key = find_key(&props, &["status", "state"])?;
    let value = enum_values(&props[&key]).into_iter().find(|v| {
        ["done", "completed", "complete", "finished"].contains(&v.to_lowercase().as_str())
    })?;
    Some((key, Value::String(value)))
}

/// The inputs for marking task `id` done.
pub fn complete_args(tool: &Tool, id: &str, list: Option<&ListRef>) -> Result<Value, String> {
    let props = properties(tool);
    let mut args = Map::new();
    let id_key = find_key(&props, &["task_id", "taskId", "id", "task", "taskID"])
        .ok_or("Blitzit's tool doesn't say which task")?;
    args.insert(id_key, Value::String(id.to_string()));
    put_list(&mut args, &props, list);
    if score(tool, Kind::CompleteTask) < 10 {
        let (key, value) = done_field(tool).ok_or("Blitzit can't mark tasks done from here")?;
        args.insert(key, value);
    } else if let Some((key, value)) = done_field(tool) {
        if required(tool).contains(&key) {
            args.insert(key, value);
        }
    }
    check_required(tool, &args)?;
    Ok(Value::Object(args))
}

// ---------------------------------------------------------------- reading answers

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub done: bool,
    /// Today, This week, …; empty when the answer doesn't say.
    pub column: String,
    /// The list it belongs to, when the answer says.
    pub list: String,
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
        || object
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("completed_at") && !v.is_null());
    let column = string_field(
        object,
        &[
            "column",
            "column_name",
            "columnName",
            "section",
            "bucket",
            "when",
        ],
    )
    .or_else(|| (!status.is_empty() && !done).then(|| status.clone()))
    .unwrap_or_else(|| parent_key.to_string());
    let list = string_field(
        object,
        &["list_name", "listName", "list", "list_id", "listId"],
    )
    .unwrap_or_default();
    Some(Item {
        id,
        name,
        done,
        column,
        list,
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

    #[test]
    fn names_split_into_words() {
        assert_eq!(words("createTask"), vec!["create", "task"]);
        assert_eq!(words("get_all-lists"), vec!["get", "all", "lists"]);
        assert_eq!(words("blitzit.add_task"), vec!["blitzit", "add", "task"]);
    }

    #[test]
    fn the_right_tools_are_picked() {
        let tools = vec![
            tool("create_list", json!({})),
            tool("get_lists", json!({})),
            tool("list_tasks", json!({})),
            tool("create_task", json!({})),
            tool("create_subtask", json!({})),
            tool(
                "update_task",
                json!({"properties": {"task_id": {}, "completed": {"type": "boolean"}}}),
            ),
            tool("delete_task", json!({})),
        ];
        assert_eq!(pick(&tools, Kind::CreateTask).unwrap().name, "create_task");
        assert_eq!(pick(&tools, Kind::Lists).unwrap().name, "get_lists");
        assert_eq!(pick(&tools, Kind::Tasks).unwrap().name, "list_tasks");
        assert_eq!(
            pick(&tools, Kind::CompleteTask).unwrap().name,
            "update_task"
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
                    "column": {"type": "string", "enum": ["BACKLOG", "THIS_WEEK", "TODAY", "DONE"]}
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
