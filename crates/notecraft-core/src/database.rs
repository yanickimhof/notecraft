//! Databases: a folder with `schema.json` (properties and views) and one
//! Markdown file per row. Also the query engine (filter, sort, group).

use crate::blocks::{self, Block};
use crate::{frontmatter, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Property types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PropKind {
    Title,
    Text,
    Number,
    Select,
    MultiSelect,
    Status,
    Date,
    Checkbox,
    Url,
    Email,
    Phone,
    CreatedTime,
    EditedTime,
    Relation,
    /// Computed from the other properties of the row (see [`crate::formula`]).
    Formula,
    /// Summary of a property of related rows.
    Rollup,
    /// People, kept as a list of names (a personal workspace has no accounts).
    Person,
    /// Files stored in the workspace `assets/` folder, kept as a list of relative paths.
    Files,
    /// A number given to each row on creation, never changed and never reused.
    UniqueId,
}

impl PropKind {
    /// Every kind a user can add, in menu order.
    pub const ADDABLE: [PropKind; 18] = [
        PropKind::Text,
        PropKind::Number,
        PropKind::Select,
        PropKind::MultiSelect,
        PropKind::Status,
        PropKind::Date,
        PropKind::Checkbox,
        PropKind::Url,
        PropKind::Email,
        PropKind::Phone,
        PropKind::CreatedTime,
        PropKind::EditedTime,
        PropKind::Relation,
        PropKind::Formula,
        PropKind::Rollup,
        PropKind::Person,
        PropKind::Files,
        PropKind::UniqueId,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PropKind::Title => "title",
            PropKind::Text => "text",
            PropKind::Number => "number",
            PropKind::Select => "select",
            PropKind::MultiSelect => "multi_select",
            PropKind::Status => "status",
            PropKind::Date => "date",
            PropKind::Checkbox => "checkbox",
            PropKind::Url => "url",
            PropKind::Email => "email",
            PropKind::Phone => "phone",
            PropKind::CreatedTime => "created_time",
            PropKind::EditedTime => "edited_time",
            PropKind::Relation => "relation",
            PropKind::Formula => "formula",
            PropKind::Rollup => "rollup",
            PropKind::Person => "person",
            PropKind::Files => "files",
            PropKind::UniqueId => "unique_id",
        }
    }

    /// Values computed from the row itself, not editable.
    pub fn is_computed(self) -> bool {
        matches!(
            self,
            PropKind::CreatedTime | PropKind::EditedTime | PropKind::Formula | PropKind::Rollup | PropKind::UniqueId
        )
    }

    /// Kinds that hold a list of named choices (people are chosen like tags).
    pub fn is_multi_choice(self) -> bool {
        matches!(self, PropKind::MultiSelect | PropKind::Person)
    }

    /// Kinds a board can group by.
    pub fn groupable(self) -> bool {
        matches!(
            self,
            PropKind::Select | PropKind::MultiSelect | PropKind::Person | PropKind::Status | PropKind::Checkbox
        )
    }

    pub fn has_options(self) -> bool {
        matches!(self, PropKind::Select | PropKind::MultiSelect | PropKind::Person | PropKind::Status)
    }
}

/// Status groups, as in "Not started / In progress / Done".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum StatusGroup {
    ToDo,
    InProgress,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub name: String,
    /// Colour key: default, gray, brown, orange, yellow, green, blue, purple, pink, red.
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<StatusGroup>,
}

fn default_color() -> String {
    "default".into()
}

/// Option colours offered in menus.
pub const COLORS: [&str; 10] =
    ["default", "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub id: String,
    pub name: String,
    pub kind: PropKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    /// For relations: the id of the related database.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_db: Option<String>,
    /// For formulas: the expression.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub formula: String,
    /// For rollups: what to summarise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollup: Option<crate::formula::Rollup>,
}

impl Property {
    pub fn new(name: &str, kind: PropKind) -> Self {
        let mut p = Property {
            id: crate::ids::new_id(),
            name: name.to_string(),
            kind,
            options: Vec::new(),
            relation_db: None,
            formula: String::new(),
            rollup: None,
        };
        if kind == PropKind::Status {
            p.options = default_status_options();
        }
        p
    }

    pub fn option(&self, name: &str) -> Option<&SelectOption> {
        self.options.iter().find(|o| o.name == name)
    }
}

/// The three default status options.
pub fn default_status_options() -> Vec<SelectOption> {
    vec![
        SelectOption { name: "Not started".into(), color: "gray".into(), group: Some(StatusGroup::ToDo) },
        SelectOption { name: "In progress".into(), color: "blue".into(), group: Some(StatusGroup::InProgress) },
        SelectOption { name: "Done".into(), color: "green".into(), group: Some(StatusGroup::Complete) },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Table,
    Board,
    List,
    Gallery,
    Calendar,
    Chart,
    Timeline,
    /// Totals and distributions of the rows that pass the view's filters.
    Dashboard,
}

/// How a chart view draws its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChartStyle {
    #[default]
    Bar,
    Line,
    Donut,
}

/// What a chart view measures for each group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChartFunc {
    #[default]
    Count,
    Sum,
    Average,
}

/// Settings of a chart view; the X axis is the view's `group_by`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChartSettings {
    #[serde(default)]
    pub style: ChartStyle,
    #[serde(default)]
    pub func: ChartFunc,
    /// Number property summed or averaged (unused by `Count`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl ChartSettings {
    fn is_default(&self) -> bool {
        *self == ChartSettings::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Contains,
    NotContains,
    Is,
    IsNot,
    IsEmpty,
    IsNotEmpty,
    Greater,
    Less,
    Checked,
    Unchecked,
}

impl FilterOp {
    pub const ALL: [FilterOp; 10] = [
        FilterOp::Contains,
        FilterOp::NotContains,
        FilterOp::Is,
        FilterOp::IsNot,
        FilterOp::IsEmpty,
        FilterOp::IsNotEmpty,
        FilterOp::Greater,
        FilterOp::Less,
        FilterOp::Checked,
        FilterOp::Unchecked,
    ];
    pub fn key(self) -> &'static str {
        match self {
            FilterOp::Contains => "contains",
            FilterOp::NotContains => "not_contains",
            FilterOp::Is => "is",
            FilterOp::IsNot => "is_not",
            FilterOp::IsEmpty => "is_empty",
            FilterOp::IsNotEmpty => "is_not_empty",
            FilterOp::Greater => "greater",
            FilterOp::Less => "less",
            FilterOp::Checked => "checked",
            FilterOp::Unchecked => "unchecked",
        }
    }
    pub fn needs_value(self) -> bool {
        !matches!(self, FilterOp::IsEmpty | FilterOp::IsNotEmpty | FilterOp::Checked | FilterOp::Unchecked)
    }
    /// Operators that make sense for a property kind.
    pub fn for_kind(kind: PropKind) -> Vec<FilterOp> {
        use FilterOp::*;
        match kind {
            PropKind::Checkbox => vec![Checked, Unchecked],
            PropKind::Number
            | PropKind::Date
            | PropKind::CreatedTime
            | PropKind::EditedTime
            | PropKind::Formula
            | PropKind::Rollup
            | PropKind::UniqueId => {
                vec![Is, IsNot, Greater, Less, IsEmpty, IsNotEmpty]
            }
            PropKind::Select | PropKind::Status => vec![Is, IsNot, IsEmpty, IsNotEmpty],
            PropKind::MultiSelect | PropKind::Person | PropKind::Files | PropKind::Relation => {
                vec![Contains, NotContains, IsEmpty, IsNotEmpty]
            }
            _ => vec![Contains, NotContains, Is, IsNot, IsEmpty, IsNotEmpty],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    /// Property id.
    pub prop: String,
    pub op: FilterOp,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    #[default]
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FilterGroup {
    #[serde(default)]
    pub mode: FilterMode,
    #[serde(default)]
    pub rules: Vec<Filter>,
    /// Nested groups, combined with the rules by this group's mode (files written before groups
    /// existed have none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<FilterGroup>,
}

impl FilterGroup {
    /// Number of rules in this group and all the groups inside it.
    pub fn count(&self) -> usize {
        self.rules.len() + self.groups.iter().map(FilterGroup::count).sum::<usize>()
    }

    /// Drop every rule that uses a property (it was deleted), and groups left empty.
    pub fn remove_prop(&mut self, prop: &str) {
        self.rules.retain(|f| f.prop != prop);
        for g in &mut self.groups {
            g.remove_prop(prop);
        }
        self.groups.retain(|g| g.count() > 0);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sort {
    pub prop: String,
    #[serde(default)]
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub id: String,
    pub name: String,
    pub kind: ViewKind,
    #[serde(default)]
    pub filter: FilterGroup,
    #[serde(default)]
    pub sorts: Vec<Sort>,
    /// Property id to group by (boards require one).
    #[serde(default)]
    pub group_by: Option<String>,
    /// Visible property ids in order; empty = all properties.
    #[serde(default)]
    pub visible: Vec<String>,
    /// Column widths for the table view (property id → points).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub widths: BTreeMap<String, f32>,
    /// Only used by chart views.
    #[serde(default, skip_serializing_if = "ChartSettings::is_default")]
    pub chart: ChartSettings,
    /// Property id of the end date of a timeline (the start is `group_by`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_by: Option<String>,
}

impl View {
    pub fn new(name: &str, kind: ViewKind) -> Self {
        View {
            id: crate::ids::new_id(),
            name: name.to_string(),
            kind,
            filter: FilterGroup::default(),
            sorts: Vec::new(),
            group_by: None,
            visible: Vec::new(),
            widths: BTreeMap::new(),
            chart: ChartSettings::default(),
            end_by: None,
        }
    }
}

fn default_format() -> u32 {
    1
}

/// `schema.json` of a database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    /// File format version (see [`crate::FORMAT_VERSION`]); a file without it is format 1.
    #[serde(default = "default_format")]
    pub format: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub order: f64,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub trashed: Option<String>,
    /// Protected against edits, moves and deletion (it also protects everything inside it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    pub properties: Vec<Property>,
    pub views: Vec<View>,
}

impl Schema {
    pub fn prop(&self, id: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.id == id)
    }
    pub fn prop_by_name(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name == name)
    }
    pub fn title_prop(&self) -> Option<&Property> {
        self.properties.iter().find(|p| p.kind == PropKind::Title)
    }
    /// Visible properties of a view, in order.
    pub fn visible_props(&self, view: &View) -> Vec<&Property> {
        if view.visible.is_empty() {
            return self.properties.iter().collect();
        }
        let mut v: Vec<&Property> = view.visible.iter().filter_map(|id| self.prop(id)).collect();
        // the title is always shown
        if let Some(t) = self.title_prop() {
            if !v.iter().any(|p| p.id == t.id) {
                v.insert(0, t);
            }
        }
        v
    }
}

/// Front matter of a row file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RowMeta {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cover: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    /// Property values keyed by property *name* (readable in git diffs).
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed: Option<String>,
    /// Protected against edits, moves and deletion (it also protects everything inside it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub meta: RowMeta,
    pub blocks: Vec<Block>,
    pub path: Option<PathBuf>,
    /// Values of formula and rollup properties by property id; never written to disk.
    pub computed: BTreeMap<String, PropValue>,
}

impl Row {
    pub fn new(title: &str) -> Self {
        let now = crate::now_string();
        Row {
            meta: RowMeta {
                id: crate::ids::new_id(),
                title: title.to_string(),
                created: now.clone(),
                updated: now,
                ..Default::default()
            },
            blocks: Vec::new(),
            path: None,
            computed: BTreeMap::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Row> {
        let (yaml, body) = frontmatter::split(text);
        let mut meta: RowMeta = match yaml {
            Some(y) if !y.trim().is_empty() => serde_yaml_ng::from_str(y)?,
            _ => RowMeta::default(),
        };
        if meta.id.is_empty() {
            meta.id = crate::ids::new_id();
        }
        Ok(Row { meta, blocks: blocks::from_markdown(body), path: None, computed: BTreeMap::new() })
    }

    pub fn render(&self) -> Result<String> {
        let yaml = format!("format: {}\n{}", crate::FORMAT_VERSION, serde_yaml_ng::to_string(&self.meta)?);
        Ok(frontmatter::join(&yaml, &blocks::to_markdown(&self.blocks)))
    }

    pub fn touch(&mut self) {
        self.meta.updated = crate::now_string();
    }

    /// Typed value of a property.
    pub fn value(&self, prop: &Property) -> PropValue {
        match prop.kind {
            PropKind::Title => PropValue::Text(self.meta.title.clone()),
            PropKind::CreatedTime => PropValue::Date(self.meta.created.clone()),
            PropKind::EditedTime => PropValue::Date(self.meta.updated.clone()),
            PropKind::Formula | PropKind::Rollup => self.computed.get(&prop.id).cloned().unwrap_or(PropValue::Empty),
            _ => PropValue::from_json(prop.kind, self.meta.properties.get(&prop.name)),
        }
    }

    /// Set a property value (computed properties are ignored).
    pub fn set_value(&mut self, prop: &Property, value: PropValue) {
        match prop.kind {
            PropKind::Title => self.meta.title = value.display(),
            k if k.is_computed() => {}
            _ => {
                if value.is_empty() {
                    self.meta.properties.remove(&prop.name);
                } else {
                    self.meta.properties.insert(prop.name.clone(), value.to_json());
                }
            }
        }
    }
}

/// A typed property value.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Empty,
    Text(String),
    Number(f64),
    Select(String),
    Multi(Vec<String>),
    /// ISO date `YYYY-MM-DD` or an RFC 3339 timestamp.
    Date(String),
    Checkbox(bool),
}

impl PropValue {
    pub fn from_json(kind: PropKind, v: Option<&Value>) -> PropValue {
        let v = match v {
            None | Some(Value::Null) => {
                return if kind == PropKind::Checkbox { PropValue::Checkbox(false) } else { PropValue::Empty }
            }
            Some(v) => v,
        };
        let as_text = || match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            other => other.to_string(),
        };
        let as_list = || match v {
            Value::Array(a) => a
                .iter()
                .map(|x| match x {
                    Value::String(s) => s.clone(),
                    o => o.to_string(),
                })
                .collect::<Vec<_>>(),
            Value::String(s) if s.is_empty() => vec![],
            Value::String(s) => s.split(',').map(|x| x.trim().to_string()).collect(),
            _ => vec![],
        };
        match kind {
            PropKind::Number | PropKind::UniqueId => match v {
                Value::Number(n) => n.as_f64().map(PropValue::Number).unwrap_or(PropValue::Empty),
                Value::String(s) => s.trim().parse().map(PropValue::Number).unwrap_or(PropValue::Empty),
                _ => PropValue::Empty,
            },
            PropKind::Checkbox => PropValue::Checkbox(match v {
                Value::Bool(b) => *b,
                Value::String(s) => matches!(s.as_str(), "true" | "yes" | "x" | "1"),
                Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                _ => false,
            }),
            PropKind::Select | PropKind::Status => PropValue::Select(as_text()),
            PropKind::MultiSelect | PropKind::Person | PropKind::Files | PropKind::Relation => {
                PropValue::Multi(as_list())
            }
            PropKind::Date | PropKind::CreatedTime | PropKind::EditedTime => PropValue::Date(as_text()),
            _ => PropValue::Text(as_text()),
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            PropValue::Empty => Value::Null,
            PropValue::Text(s) | PropValue::Select(s) | PropValue::Date(s) => Value::String(s.clone()),
            PropValue::Number(n) => serde_json::Number::from_f64(*n).map(Value::Number).unwrap_or(Value::Null),
            PropValue::Multi(v) => Value::Array(v.iter().cloned().map(Value::String).collect()),
            PropValue::Checkbox(b) => Value::Bool(*b),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            PropValue::Empty => true,
            PropValue::Text(s) | PropValue::Select(s) | PropValue::Date(s) => s.trim().is_empty(),
            PropValue::Multi(v) => v.is_empty(),
            PropValue::Number(_) => false,
            PropValue::Checkbox(b) => !*b,
        }
    }

    /// Plain text for display, search and CSV.
    pub fn display(&self) -> String {
        match self {
            PropValue::Empty => String::new(),
            PropValue::Text(s) | PropValue::Select(s) => s.clone(),
            PropValue::Date(s) => s.clone(),
            PropValue::Number(n) => format_number(*n),
            PropValue::Multi(v) => v.join(", "),
            PropValue::Checkbox(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
        }
    }
}

/// `3` instead of `3.0`, otherwise up to 6 decimals without trailing zeros.
pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A loaded database: schema, rows and templates.
#[derive(Debug, Clone, PartialEq)]
pub struct Database {
    pub schema: Schema,
    pub rows: BTreeMap<String, Row>,
    pub templates: BTreeMap<String, Row>,
    /// Folder of this database (absolute).
    pub dir: PathBuf,
}

impl Database {
    /// Give a number to every row that has none, in each unique-id property (oldest rows first,
    /// continuing after the highest number in use). Returns the ids of the rows that changed.
    pub fn assign_unique_ids(&mut self) -> Vec<String> {
        let names: Vec<String> =
            self.schema.properties.iter().filter(|p| p.kind == PropKind::UniqueId).map(|p| p.name.clone()).collect();
        let mut changed = Vec::new();
        for name in names {
            let mut next = self
                .rows
                .values()
                .filter_map(|r| r.meta.properties.get(&name).and_then(|v| v.as_f64()))
                .fold(0.0_f64, f64::max) as u64
                + 1;
            let mut missing: Vec<(String, String)> = self
                .rows
                .iter()
                .filter(|(_, r)| r.meta.properties.get(&name).and_then(|v| v.as_f64()).is_none())
                .map(|(id, r)| (r.meta.created.clone(), id.clone()))
                .collect();
            missing.sort();
            for (_, id) in missing {
                if let Some(r) = self.rows.get_mut(&id) {
                    r.meta.properties.insert(name.clone(), Value::from(next));
                    next += 1;
                    changed.push(id);
                }
            }
        }
        changed.sort();
        changed.dedup();
        changed
    }

    /// A new database with a title property, tags, status and date, and a table view.
    pub fn new_schema(id: String, title: &str, parent: Option<String>) -> Schema {
        let now = crate::now_string();
        let mut tags = Property::new("Tags", PropKind::MultiSelect);
        tags.options = vec![
            SelectOption { name: "Idea".into(), color: "yellow".into(), group: None },
            SelectOption { name: "Work".into(), color: "blue".into(), group: None },
            SelectOption { name: "Personal".into(), color: "pink".into(), group: None },
        ];
        Schema {
            format: crate::FORMAT_VERSION,
            id,
            title: title.to_string(),
            icon: String::new(),
            cover: String::new(),
            description: String::new(),
            parent,
            order: 0.0,
            favorite: false,
            trashed: None,
            locked: false,
            created: now.clone(),
            updated: now,
            properties: vec![
                Property::new("Name", PropKind::Title),
                Property::new("Status", PropKind::Status),
                tags,
                Property::new("Date", PropKind::Date),
            ],
            views: vec![View::new("Table", ViewKind::Table)],
        }
    }

    pub fn view(&self, id: &str) -> Option<&View> {
        self.schema.views.iter().find(|v| v.id == id)
    }

    /// Rows that are not in the trash.
    pub fn live_rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.values().filter(|r| r.meta.trashed.is_none())
    }
}

// ---------------------------------------------------------------------------
// Query engine

/// One group of a query result (a board column or a table group).
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Option name / `true` / `false`; `None` = rows without a value.
    pub key: Option<String>,
    pub rows: Vec<String>,
}

fn matches_filter(row: &Row, prop: &Property, f: &Filter) -> bool {
    let v = row.value(prop);
    let needle = f.value.trim().to_lowercase();
    let text = v.display().to_lowercase();
    let num = |s: &str| s.trim().parse::<f64>().ok();
    let cmp = || -> Option<Ordering> {
        match &v {
            PropValue::Number(n) => n.partial_cmp(&num(&f.value)?),
            PropValue::Date(d) => {
                Some(d.as_str()[..d.len().min(10)].cmp(&f.value.trim()[..f.value.trim().len().min(10)]))
            }
            _ => Some(text.as_str().cmp(needle.as_str())),
        }
    };
    match f.op {
        FilterOp::Contains => match &v {
            PropValue::Multi(items) => items.iter().any(|i| i.to_lowercase() == needle) || text.contains(&needle),
            _ => text.contains(&needle),
        },
        FilterOp::NotContains => match &v {
            PropValue::Multi(items) => !items.iter().any(|i| i.to_lowercase() == needle),
            _ => !text.contains(&needle),
        },
        FilterOp::Is => match &v {
            PropValue::Number(n) => num(&f.value) == Some(*n),
            PropValue::Date(d) => d.starts_with(f.value.trim()),
            _ => text == needle,
        },
        FilterOp::IsNot => match &v {
            PropValue::Number(n) => num(&f.value) != Some(*n),
            PropValue::Date(d) => !d.starts_with(f.value.trim()),
            _ => text != needle,
        },
        FilterOp::IsEmpty => v.is_empty(),
        FilterOp::IsNotEmpty => !v.is_empty(),
        FilterOp::Greater => !v.is_empty() && cmp() == Some(Ordering::Greater),
        FilterOp::Less => !v.is_empty() && cmp() == Some(Ordering::Less),
        FilterOp::Checked => v == PropValue::Checkbox(true),
        FilterOp::Unchecked => v != PropValue::Checkbox(true),
    }
}

/// Does a row pass the view's filters?
pub fn row_passes(schema: &Schema, row: &Row, group: &FilterGroup) -> bool {
    let mut results: Vec<bool> =
        group.rules.iter().filter_map(|f| schema.prop(&f.prop).map(|p| matches_filter(row, p, f))).collect();
    // an empty nested group says nothing, so it neither lets rows through nor hides them
    results.extend(group.groups.iter().filter(|g| g.count() > 0).map(|g| row_passes(schema, row, g)));
    if results.is_empty() {
        return true;
    }
    match group.mode {
        FilterMode::And => results.iter().all(|r| *r),
        FilterMode::Or => results.iter().any(|r| *r),
    }
}

fn compare_values(prop: &Property, a: &PropValue, b: &PropValue) -> Ordering {
    // empty values sort last
    match (a.is_empty() && !matches!(a, PropValue::Checkbox(_)), b.is_empty() && !matches!(b, PropValue::Checkbox(_))) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (true, true) => return Ordering::Equal,
        _ => {}
    }
    match (a, b) {
        (PropValue::Number(x), PropValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (PropValue::Checkbox(x), PropValue::Checkbox(y)) => x.cmp(y),
        (PropValue::Select(x), PropValue::Select(y)) if prop.kind.has_options() => {
            let pos = |n: &str| prop.options.iter().position(|o| o.name == n).unwrap_or(usize::MAX);
            pos(x).cmp(&pos(y))
        }
        _ => a.display().to_lowercase().cmp(&b.display().to_lowercase()),
    }
}

/// Filter and sort a database's rows for a view. Rows without explicit sorts
/// keep their creation order.
pub fn sorted_rows(db: &Database, view: &View) -> Vec<String> {
    let mut rows: Vec<&Row> = db.live_rows().filter(|r| row_passes(&db.schema, r, &view.filter)).collect();
    rows.sort_by(|a, b| {
        for s in &view.sorts {
            if let Some(p) = db.schema.prop(&s.prop) {
                let mut o = compare_values(p, &a.value(p), &b.value(p));
                if s.descending {
                    o = o.reverse();
                }
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
        a.meta.created.cmp(&b.meta.created).then(a.meta.id.cmp(&b.meta.id))
    });
    rows.into_iter().map(|r| r.meta.id.clone()).collect()
}

/// The property a view groups by (boards fall back to the first status or select).
/// The date property a calendar view is laid out on: the chosen one, else the first date property.
pub fn date_prop<'a>(schema: &'a Schema, view: &View) -> Option<&'a Property> {
    view.group_by
        .as_ref()
        .and_then(|id| schema.prop(id))
        .filter(|p| p.kind == PropKind::Date)
        .or_else(|| schema.properties.iter().find(|p| p.kind == PropKind::Date))
}

pub fn group_prop<'a>(schema: &'a Schema, view: &View) -> Option<&'a Property> {
    if matches!(
        view.kind,
        ViewKind::Calendar | ViewKind::Gallery | ViewKind::Chart | ViewKind::Timeline | ViewKind::Dashboard
    ) {
        return None;
    }
    if let Some(id) = &view.group_by {
        if let Some(p) = schema.prop(id) {
            return Some(p);
        }
    }
    if view.kind == ViewKind::Board {
        return schema
            .properties
            .iter()
            .find(|p| p.kind == PropKind::Status)
            .or_else(|| schema.properties.iter().find(|p| p.kind == PropKind::Select));
    }
    None
}

/// A row placed on a timeline: first and last day (inclusive).
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineItem {
    pub row: String,
    pub start: jiff::civil::Date,
    pub end: jiff::civil::Date,
}

fn day_of(v: &PropValue) -> Option<jiff::civil::Date> {
    match v {
        PropValue::Date(s) => s.get(..10).and_then(|d| d.parse().ok()),
        _ => None,
    }
}

/// The end date property of a timeline view, if one is chosen and exists.
pub fn end_prop<'a>(schema: &'a Schema, view: &View) -> Option<&'a Property> {
    view.end_by.as_ref().and_then(|id| schema.prop(id)).filter(|p| p.kind == PropKind::Date)
}

/// Rows of a timeline, ordered by start date, and the rows that have no start date.
/// A missing or earlier end date makes a one day item.
pub fn timeline_items(db: &Database, view: &View) -> (Vec<TimelineItem>, Vec<String>) {
    let Some(start_p) = date_prop(&db.schema, view) else { return (Vec::new(), Vec::new()) };
    let end_p = end_prop(&db.schema, view).filter(|p| p.id != start_p.id);
    let mut items = Vec::new();
    let mut undated = Vec::new();
    for g in query(db, view) {
        for rid in g.rows {
            let row = &db.rows[&rid];
            match day_of(&row.value(start_p)) {
                Some(start) => {
                    let end = end_p.and_then(|p| day_of(&row.value(p))).filter(|e| *e >= start).unwrap_or(start);
                    items.push(TimelineItem { row: rid, start, end });
                }
                None => undated.push(rid),
            }
        }
    }
    items.sort_by_key(|i| (i.start, i.end));
    (items, undated)
}

/// The property on the X axis of a chart: the chosen one, else the first status, select, tag, checkbox or date.
pub fn chart_x<'a>(schema: &'a Schema, view: &View) -> Option<&'a Property> {
    view.group_by.as_ref().and_then(|id| schema.prop(id)).or_else(|| {
        [PropKind::Status, PropKind::Select, PropKind::MultiSelect, PropKind::Checkbox, PropKind::Date]
            .into_iter()
            .find_map(|k| schema.properties.iter().find(|p| p.kind == k))
    })
}

/// Points of a chart view: one `(label, value)` per group, after the view's filters. A group
/// without value has the label `""`. Dates are grouped by month.
/// What a number property adds up to over the rows of a dashboard.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberStat {
    pub prop: String,
    pub filled: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

impl NumberStat {
    pub fn average(&self) -> f64 {
        if self.filled == 0 {
            0.0
        } else {
            self.sum / self.filled as f64
        }
    }
}

/// How many rows hold each value of a choice property.
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    pub prop: String,
    pub counts: Vec<(String, usize)>,
}

/// The figures a dashboard view shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DashboardData {
    pub total: usize,
    pub numbers: Vec<NumberStat>,
    pub distributions: Vec<Distribution>,
    /// Checkbox property name and how many rows have it ticked.
    pub checks: Vec<(String, usize)>,
}

/// Totals over the rows that pass the view's filters: counts per choice, sums of numbers, ticked boxes.
pub fn dashboard_data(db: &Database, view: &View) -> DashboardData {
    let ids = sorted_rows(db, view);
    let mut out = DashboardData { total: ids.len(), ..Default::default() };
    for p in &db.schema.properties {
        match p.kind {
            PropKind::Number | PropKind::Formula | PropKind::Rollup => {
                let nums: Vec<f64> = ids
                    .iter()
                    .filter_map(|id| match db.rows[id].value(p) {
                        PropValue::Number(n) => Some(n),
                        _ => None,
                    })
                    .collect();
                if !nums.is_empty() {
                    out.numbers.push(NumberStat {
                        prop: p.name.clone(),
                        filled: nums.len(),
                        sum: nums.iter().sum(),
                        min: nums.iter().cloned().fold(f64::INFINITY, f64::min),
                        max: nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    });
                }
            }
            PropKind::Select | PropKind::Status | PropKind::MultiSelect | PropKind::Person => {
                let mut counts: Vec<(String, usize)> = p.options.iter().map(|o| (o.name.clone(), 0)).collect();
                for id in &ids {
                    let names = match db.rows[id].value(p) {
                        PropValue::Select(s) if !s.is_empty() => vec![s],
                        PropValue::Multi(m) => m,
                        _ => vec![],
                    };
                    for n in names {
                        match counts.iter_mut().find(|c| c.0 == n) {
                            Some(c) => c.1 += 1,
                            None => counts.push((n, 1)),
                        }
                    }
                }
                counts.retain(|c| c.1 > 0);
                if !counts.is_empty() {
                    out.distributions.push(Distribution { prop: p.name.clone(), counts });
                }
            }
            PropKind::Checkbox => {
                let n = ids.iter().filter(|id| db.rows[*id].value(p) == PropValue::Checkbox(true)).count();
                out.checks.push((p.name.clone(), n));
            }
            _ => {}
        }
    }
    out
}

pub fn chart_data(db: &Database, view: &View) -> Vec<(String, f64)> {
    let Some(x) = chart_x(&db.schema, view) else { return Vec::new() };
    let value_prop = view.chart.value.as_ref().and_then(|id| db.schema.prop(id));
    let mut preset: Vec<String> = match x.kind {
        PropKind::Checkbox => vec!["false".into(), "true".into()],
        k if k.has_options() => x.options.iter().map(|o| o.name.clone()).collect(),
        _ => Vec::new(),
    };
    // label, sum of values, rows, rows with a number
    let mut acc: Vec<(String, f64, usize, usize)> = preset.iter().map(|k| (k.clone(), 0.0, 0, 0)).collect();
    for id in sorted_rows(db, view) {
        let row = &db.rows[&id];
        let keys: Vec<String> = match row.value(x) {
            PropValue::Checkbox(b) => vec![b.to_string()],
            PropValue::Multi(m) if !m.is_empty() => m,
            PropValue::Date(d) if !d.trim().is_empty() => vec![d.chars().take(7).collect()],
            v if v.is_empty() => vec![String::new()],
            v => vec![v.display()],
        };
        let num = value_prop.and_then(|p| match row.value(p) {
            PropValue::Number(n) => Some(n),
            _ => None,
        });
        for k in keys {
            let i = match acc.iter().position(|a| a.0 == k) {
                Some(i) => i,
                None => {
                    acc.push((k, 0.0, 0, 0));
                    acc.len() - 1
                }
            };
            acc[i].2 += 1;
            if let Some(n) = num {
                acc[i].1 += n;
                acc[i].3 += 1;
            }
        }
    }
    if preset.is_empty() {
        acc.sort_by(|a, b| (a.0.is_empty(), &a.0).cmp(&(b.0.is_empty(), &b.0)));
    } else {
        // the preset order first, other labels and the empty group after
        preset.push(String::new());
        acc.sort_by_key(|a| preset.iter().position(|k| *k == a.0).unwrap_or(preset.len()));
        acc.retain(|a| !a.0.is_empty() || a.2 > 0);
    }
    acc.into_iter()
        .map(|(k, sum, n, nn)| {
            let v = match view.chart.func {
                ChartFunc::Count => n as f64,
                ChartFunc::Sum => sum,
                ChartFunc::Average if nn > 0 => sum / nn as f64,
                ChartFunc::Average => 0.0,
            };
            (k, v)
        })
        .collect()
}

/// Run a view: filter, sort, then group. Without grouping there is one group
/// with key `None`.
pub fn query(db: &Database, view: &View) -> Vec<Group> {
    let ids = sorted_rows(db, view);
    let prop = match group_prop(&db.schema, view) {
        Some(p) => p,
        None => return vec![Group { key: None, rows: ids }],
    };
    let mut keys: Vec<Option<String>> = vec![None];
    match prop.kind {
        PropKind::Checkbox => {
            keys = vec![Some("false".into()), Some("true".into())];
        }
        k if k.has_options() => {
            let mut opts: Vec<&SelectOption> = prop.options.iter().collect();
            if k == PropKind::Status {
                opts.sort_by_key(|o| o.group.unwrap_or(StatusGroup::ToDo));
            }
            keys.extend(opts.into_iter().map(|o| Some(o.name.clone())));
        }
        _ => {}
    }
    let mut groups: Vec<Group> = keys.into_iter().map(|key| Group { key, rows: vec![] }).collect();
    for id in ids {
        let row = &db.rows[&id];
        let v = row.value(prop);
        let row_keys: Vec<Option<String>> = match v {
            PropValue::Checkbox(b) => vec![Some(b.to_string())],
            PropValue::Multi(items) if !items.is_empty() => items.into_iter().map(Some).collect(),
            PropValue::Empty | PropValue::Multi(_) => vec![None],
            other if other.is_empty() => vec![None],
            other => vec![Some(other.display())],
        };
        for k in row_keys {
            match groups.iter_mut().find(|g| g.key == k) {
                Some(g) => g.rows.push(id.clone()),
                None => groups.push(Group { key: k, rows: vec![id.clone()] }),
            }
        }
    }
    groups
}

#[cfg(test)]
mod tests;
