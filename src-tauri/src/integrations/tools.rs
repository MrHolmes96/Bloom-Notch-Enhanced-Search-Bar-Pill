//! Finding the right tool on an app's MCP server and filling in its inputs
//! from the tool's own description (its name, its text and its JSON schema),
//! so Bloom keeps working whatever the app calls its tools. The choice can
//! also be made by hand (Settings > Integrations), which always wins.

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

impl Kind {
    pub const ALL: [Kind; 4] = [
        Kind::CreateTask,
        Kind::Lists,
        Kind::Tasks,
        Kind::CompleteTask,
    ];

    /// The name used for a hand-made choice in the settings.
    pub fn id(self) -> &'static str {
        match self {
            Kind::CreateTask => "create",
            Kind::Lists => "lists",
            Kind::Tasks => "tasks",
            Kind::CompleteTask => "complete",
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
                10 + if has(&name, &["tasks"]) { 2 } else { 0 }
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
        Kind::CompleteTask => {
            if !task_name && !task_text {
                return 0;
            }
            if destroy
                || has(
                    &name,
                    &["subtask", "uncomplete", "incomplete", "undo", "reopen"],
                )
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
            has(&w, &["list", "listid", "project", "board"]) && !has(&w, &["column", "lists"])
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
    let props = properties(tool);
    let mut args = Map::new();
    if let Some((key, value)) = action_value(&props, READ) {
        args.insert(key, Value::String(value));
    }
    put_list(&mut args, &props, list);
    check_required(&tool.schema, &args)?;
    Ok(Value::Object(args))
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
    let id_key = find_key(
        &props,
        &[
            "task_id", "taskId", "id", "task", "taskID", "item_id", "itemId",
        ],
    )
    .ok_or("Blitzit's tool doesn't say which task")?;
    args.insert(id_key, Value::String(id.to_string()));
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

    fn described(name: &str, description: &str, schema: Value) -> Tool {
        Tool {
            name: name.into(),
            description: description.into(),
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
