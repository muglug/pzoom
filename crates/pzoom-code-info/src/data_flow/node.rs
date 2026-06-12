use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use pzoom_str::{Interner, StrId};
use serde::{Deserialize, Serialize};

use crate::method_identifier::MethodIdentifier;

pub(crate) trait IntoStrId {
    fn into_str_id(self) -> StrId;
}

impl IntoStrId for StrId {
    fn into_str_id(self) -> StrId {
        self
    }
}

impl IntoStrId for &StrId {
    fn into_str_id(self) -> StrId {
        *self
    }
}

pub(crate) fn lookup_id(interner: &Interner, id: impl IntoStrId) -> Arc<str> {
    interner.lookup(id.into_str_id())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub struct VarId(pub StrId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum FunctionLikeIdentifier {
    Function(StrId),
    Method(StrId, StrId),
    Closure(StrId, u32),
}

impl FunctionLikeIdentifier {
    pub fn as_method_identifier(&self) -> Option<MethodIdentifier> {
        if let FunctionLikeIdentifier::Method(classlike_name, method_name) = self {
            Some(MethodIdentifier(*classlike_name, *method_name))
        } else {
            None
        }
    }

    pub fn to_string(&self, interner: &Interner) -> String {
        match self {
            FunctionLikeIdentifier::Function(fn_name) => lookup_id(interner, fn_name).to_string(),
            FunctionLikeIdentifier::Method(class_name, method_name) => {
                format!(
                    "{}::{}",
                    lookup_id(interner, class_name),
                    lookup_id(interner, method_name)
                )
            }
            FunctionLikeIdentifier::Closure(file_path, offset) => {
                format!("{}:{}", lookup_id(interner, file_path), offset)
            }
        }
    }

    pub fn to_hash(&self) -> String {
        match self {
            FunctionLikeIdentifier::Function(fn_name) => fn_name.0.to_string(),
            FunctionLikeIdentifier::Method(class_name, method_name) => {
                format!("{}::{}", class_name.0, method_name.0)
            }
            FunctionLikeIdentifier::Closure(file_path, offset) => {
                format!("{}::{}", file_path.0, offset)
            }
        }
    }

    pub fn to_ref(&self) -> (StrId, StrId) {
        match self {
            FunctionLikeIdentifier::Function(fn_name) => (*fn_name, StrId::EMPTY),
            FunctionLikeIdentifier::Method(class_name, method_name) => (*class_name, *method_name),
            FunctionLikeIdentifier::Closure(file_path, offset) => (*file_path, StrId(*offset)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableSourceKind {
    Default,
    PrivateParam,
    NonPrivateParam,
    InoutParam,
    InoutArg,
    ClosureParam,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum DataFlowNodeId {
    String(String),
    LocalString(String, StrId, u32, u32),
    ArrayAssignment(StrId, u32, u32),
    ArrayItem(String, StrId, u32, u32),
    Return(StrId, u32, u32),
    ForInit(u32, u32),
    Composition(StrId, u32, u32),
    Var(VarId, StrId, u32, u32),
    VarNarrowedTo(String, StrId, StrId, u32),
    Param(VarId, StrId, u32, u32),
    UnlabelledSink(StrId, u32, u32),
    ReferenceTo(FunctionLikeIdentifier),
    CallTo(FunctionLikeIdentifier),
    SpecializedCallTo(FunctionLikeIdentifier, StrId, u32),
    FunctionLikeArg(FunctionLikeIdentifier, u8),
    SpecializedFunctionLikeArg(FunctionLikeIdentifier, u8, StrId, u32),
    Property(StrId, StrId),
    SpecializedProperty(StrId, StrId, StrId, u32, u32),
    PropertyFetch(VarId, StrId, StrId, u32),
    FunctionLikeOut(FunctionLikeIdentifier, u8),
    SpecializedFunctionLikeOut(FunctionLikeIdentifier, u8, StrId, u32),
    ThisBeforeMethod(MethodIdentifier),
    SpecializedThisBeforeMethod(MethodIdentifier, StrId, u32),
    ThisAfterMethod(MethodIdentifier),
    SpecializedThisAfterMethod(MethodIdentifier, StrId, u32),
    Symbol(StrId),
    ShapeFieldAccess(StrId, String),
    InstanceMethodCall(StrId, u32, u32),
}

impl DataFlowNodeId {
    pub fn to_string(&self, interner: &Interner) -> String {
        match self {
            DataFlowNodeId::String(str) => str.clone(),
            DataFlowNodeId::LocalString(str, file_path, start_offset, end_offset) => {
                format!(
                    "{}-{}:{}-{}",
                    str,
                    lookup_id(interner, file_path),
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::Param(var_id, file_path, start_offset, end_offset) => {
                format!(
                    "param-{}-{}:{}-{}",
                    lookup_id(interner, var_id.0),
                    lookup_id(interner, file_path),
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::Var(var_id, file_path, start_offset, end_offset) => {
                format!(
                    "{}-{}:{}-{}",
                    lookup_id(interner, var_id.0),
                    lookup_id(interner, file_path),
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::VarNarrowedTo(var_id, symbol, file_path, start_offset) => {
                format!(
                    "{} narrowed to {}-{}:{}",
                    var_id,
                    lookup_id(interner, symbol),
                    lookup_id(interner, file_path),
                    start_offset
                )
            }
            DataFlowNodeId::ArrayAssignment(file_path, start_offset, end_offset) => {
                format!(
                    "array-assignment-{}:{}-{}",
                    lookup_id(interner, file_path),
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::ArrayItem(key_value, file_path, start_offset, end_offset) => {
                format!(
                    "array[{}]-{}:{}-{}",
                    key_value,
                    lookup_id(interner, file_path),
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::Return(file_path, start_offset, end_offset) => {
                format!(
                    "return-{}:{}-{}",
                    lookup_id(interner, file_path),
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::CallTo(functionlike_id) => {
                format!("call to {}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::SpecializedCallTo(functionlike_id, file_path, start_offset) => {
                format!(
                    "call to {}-{}:{}",
                    functionlike_id.to_string(interner),
                    lookup_id(interner, file_path),
                    start_offset
                )
            }
            DataFlowNodeId::Property(classlike_name, property_name) => format!(
                "{}::${}",
                lookup_id(interner, classlike_name),
                lookup_id(interner, property_name)
            ),
            DataFlowNodeId::SpecializedProperty(
                classlike_name,
                property_name,
                file_path,
                start_offset,
                end_offset,
            ) => format!(
                "{}::${}-{}:{}-{}",
                lookup_id(interner, classlike_name),
                lookup_id(interner, property_name),
                lookup_id(interner, file_path),
                start_offset,
                end_offset
            ),
            DataFlowNodeId::FunctionLikeOut(functionlike_id, arg) => {
                format!("out {}#{}", functionlike_id.to_string(interner), arg + 1)
            }
            DataFlowNodeId::SpecializedFunctionLikeOut(
                functionlike_id,
                arg,
                file_path,
                start_offset,
            ) => {
                format!(
                    "out {}#{}-{}:{}",
                    functionlike_id.to_string(interner),
                    arg + 1,
                    lookup_id(interner, file_path),
                    start_offset
                )
            }
            DataFlowNodeId::FunctionLikeArg(functionlike_id, arg) => {
                format!("{}#{}", functionlike_id.to_string(interner), arg + 1)
            }
            DataFlowNodeId::SpecializedFunctionLikeArg(
                functionlike_id,
                arg,
                file_path,
                start_offset,
            ) => {
                format!(
                    "{}#{}-{}:{}",
                    functionlike_id.to_string(interner),
                    arg + 1,
                    lookup_id(interner, file_path),
                    start_offset
                )
            }
            DataFlowNodeId::PropertyFetch(lhs_var_id, property_name, file_path, start_offset) => {
                format!(
                    "{}->{}-{}:{}",
                    lookup_id(interner, lhs_var_id.0),
                    lookup_id(interner, property_name),
                    lookup_id(interner, file_path),
                    start_offset,
                )
            }
            DataFlowNodeId::ThisBeforeMethod(method_id) => format!(
                "$this in {} before {}",
                lookup_id(interner, method_id.0),
                lookup_id(interner, method_id.1)
            ),
            DataFlowNodeId::SpecializedThisBeforeMethod(method_id, file_path, start_offset) => {
                format!(
                    "$this in {} before {}-{}:{}",
                    lookup_id(interner, method_id.0),
                    lookup_id(interner, method_id.1),
                    lookup_id(interner, file_path),
                    start_offset,
                )
            }
            DataFlowNodeId::ThisAfterMethod(method_id) => format!(
                "$this in {} after {}",
                lookup_id(interner, method_id.0),
                lookup_id(interner, method_id.1)
            ),
            DataFlowNodeId::SpecializedThisAfterMethod(method_id, file_path, start_offset) => {
                format!(
                    "$this in {} after {}-{}:{}",
                    lookup_id(interner, method_id.0),
                    lookup_id(interner, method_id.1),
                    lookup_id(interner, file_path),
                    start_offset,
                )
            }
            DataFlowNodeId::Symbol(id) => lookup_id(interner, id).to_string(),
            DataFlowNodeId::ShapeFieldAccess(type_name, key) => {
                format!("{}[{}]", lookup_id(interner, type_name), key)
            }
            DataFlowNodeId::Composition(file_path, start_offset, end_offset) => format!(
                "composition-{}:{}-{}",
                lookup_id(interner, file_path),
                start_offset,
                end_offset
            ),
            DataFlowNodeId::ReferenceTo(functionlike_id) => {
                format!("fnref-{}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::ForInit(start_offset, end_offset) => {
                format!("for-init-{}-{}", start_offset, end_offset)
            }
            DataFlowNodeId::UnlabelledSink(file_path, start_offset, end_offset) => format!(
                "unlabelled-sink-{}:{}-{}",
                lookup_id(interner, file_path),
                start_offset,
                end_offset
            ),
            DataFlowNodeId::InstanceMethodCall(file_path, start_offset, end_offset) => format!(
                "instance-method-call-{}:{}-{}",
                lookup_id(interner, file_path),
                start_offset,
                end_offset
            ),
        }
    }

    pub fn to_label(&self, interner: &Interner) -> String {
        match self {
            DataFlowNodeId::String(str) | DataFlowNodeId::LocalString(str, ..) => str.clone(),
            DataFlowNodeId::Param(var_id, ..) | DataFlowNodeId::Var(var_id, ..) => {
                lookup_id(interner, var_id.0).to_string()
            }
            DataFlowNodeId::VarNarrowedTo(var_id, symbol, ..) => {
                format!("{} narrowed to {}", var_id, lookup_id(interner, symbol))
            }
            DataFlowNodeId::ArrayAssignment(..) => "array-assignment".to_string(),
            DataFlowNodeId::ArrayItem(key_value, ..) => {
                format!("array[{}]", key_value)
            }
            DataFlowNodeId::Return(..) => "return".to_string(),
            DataFlowNodeId::CallTo(functionlike_id)
            | DataFlowNodeId::SpecializedCallTo(functionlike_id, ..) => {
                format!("call to {}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::Property(classlike_name, property_name)
            | DataFlowNodeId::SpecializedProperty(classlike_name, property_name, ..) => format!(
                "{}::${}",
                lookup_id(interner, classlike_name),
                lookup_id(interner, property_name)
            ),
            DataFlowNodeId::FunctionLikeOut(functionlike_id, arg)
            | DataFlowNodeId::SpecializedFunctionLikeOut(functionlike_id, arg, ..) => {
                format!("out {}#{}", functionlike_id.to_string(interner), arg + 1)
            }
            DataFlowNodeId::FunctionLikeArg(functionlike_id, arg)
            | DataFlowNodeId::SpecializedFunctionLikeArg(functionlike_id, arg, ..) => {
                format!("{}#{}", functionlike_id.to_string(interner), arg + 1)
            }
            DataFlowNodeId::PropertyFetch(lhs_var_id, property_name, ..) => {
                format!(
                    "{}->{}",
                    lookup_id(interner, lhs_var_id.0),
                    lookup_id(interner, property_name),
                )
            }
            DataFlowNodeId::ThisBeforeMethod(method_id)
            | DataFlowNodeId::SpecializedThisBeforeMethod(method_id, ..) => format!(
                "$this in {} before {}",
                lookup_id(interner, method_id.0),
                lookup_id(interner, method_id.1)
            ),
            DataFlowNodeId::ThisAfterMethod(method_id)
            | DataFlowNodeId::SpecializedThisAfterMethod(method_id, ..) => format!(
                "$this in {} after {}",
                lookup_id(interner, method_id.0),
                lookup_id(interner, method_id.1)
            ),
            DataFlowNodeId::Symbol(id) => lookup_id(interner, id).to_string(),
            DataFlowNodeId::ShapeFieldAccess(type_name, key) => {
                format!("{}[{}]", lookup_id(interner, type_name), key)
            }
            DataFlowNodeId::Composition(..) => "composition".to_string(),
            DataFlowNodeId::ReferenceTo(functionlike_id) => {
                format!("fnref-{}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::ForInit(start_offset, end_offset) => {
                format!("for-init-{}-{}", start_offset, end_offset)
            }
            DataFlowNodeId::UnlabelledSink(..) => "unlabelled-sink".to_string(),
            DataFlowNodeId::InstanceMethodCall(..) => "instance method call".to_string(),
        }
    }

    pub fn specialize(&self, file_path: StrId, offset: u32) -> DataFlowNodeId {
        match self {
            DataFlowNodeId::CallTo(id) => DataFlowNodeId::SpecializedCallTo(*id, file_path, offset),
            DataFlowNodeId::FunctionLikeArg(functionlike_id, arg) => {
                DataFlowNodeId::SpecializedFunctionLikeArg(
                    *functionlike_id,
                    *arg,
                    file_path,
                    offset,
                )
            }
            DataFlowNodeId::FunctionLikeOut(functionlike_id, arg) => {
                DataFlowNodeId::SpecializedFunctionLikeOut(
                    *functionlike_id,
                    *arg,
                    file_path,
                    offset,
                )
            }
            DataFlowNodeId::ThisBeforeMethod(method_id) => {
                DataFlowNodeId::SpecializedThisBeforeMethod(*method_id, file_path, offset)
            }
            DataFlowNodeId::ThisAfterMethod(method_id) => {
                DataFlowNodeId::SpecializedThisAfterMethod(*method_id, file_path, offset)
            }
            _ => panic!("Cannot specialize node {:?}", self),
        }
    }

    pub fn unspecialize(&self) -> (DataFlowNodeId, (StrId, u32)) {
        match self {
            DataFlowNodeId::SpecializedCallTo(id, file_path, offset) => {
                (DataFlowNodeId::CallTo(*id), (*file_path, *offset))
            }
            DataFlowNodeId::SpecializedFunctionLikeArg(functionlike_id, arg, file_path, offset) => {
                (
                    DataFlowNodeId::FunctionLikeArg(*functionlike_id, *arg),
                    (*file_path, *offset),
                )
            }
            DataFlowNodeId::SpecializedFunctionLikeOut(functionlike_id, arg, file_path, offset) => {
                (
                    DataFlowNodeId::FunctionLikeOut(*functionlike_id, *arg),
                    (*file_path, *offset),
                )
            }
            DataFlowNodeId::SpecializedThisBeforeMethod(method_id, file_path, offset) => (
                DataFlowNodeId::ThisBeforeMethod(*method_id),
                (*file_path, *offset),
            ),
            DataFlowNodeId::SpecializedThisAfterMethod(method_id, file_path, offset) => (
                DataFlowNodeId::ThisAfterMethod(*method_id),
                (*file_path, *offset),
            ),
            _ => panic!("Cannot unspecialize node {:?}", self),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub struct DataFlowNodePosition {
    pub file_path: StrId,
    pub start_offset: u32,
    pub end_offset: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u16,
    pub end_column: u16,
}

impl DataFlowNodePosition {
    pub fn new(
        file_path: StrId,
        start_offset: u32,
        end_offset: u32,
        start_line: u32,
        end_line: u32,
        start_column: u16,
        end_column: u16,
    ) -> Self {
        Self {
            file_path,
            start_offset,
            end_offset,
            start_line,
            end_line,
            start_column,
            end_column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    Unknown,
}

/// Psalm's `TaintKind` taxonomy: the taint categories an input source can
/// carry and a sink can reject. (Psalm models these as a bitfield; pzoom uses
/// an enum in `Vec`s like Hakana — the sets stay tiny.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SinkType {
    Unknown,
    Callable,
    Unserialize,
    Include,
    Eval,
    Ldap,
    Sql,
    Html,
    HasQuotes,
    Shell,
    Ssrf,
    FileSink,
    Cookie,
    Header,
    Xpath,
    Sleep,
    Extract,
    UserSecret,
    SystemSecret,
    Custom(String),
}

impl SinkType {
    /// Psalm's `TaintKind::ALL_INPUT`: the taints carried by request
    /// superglobals and `@psalm-taint-source input` returns.
    pub fn all_input() -> Vec<SinkType> {
        vec![
            SinkType::Callable,
            SinkType::Unserialize,
            SinkType::Include,
            SinkType::Eval,
            SinkType::Ldap,
            SinkType::Sql,
            SinkType::Html,
            SinkType::HasQuotes,
            SinkType::Shell,
            SinkType::Ssrf,
            SinkType::FileSink,
            SinkType::Cookie,
            SinkType::Header,
            SinkType::Xpath,
            SinkType::Sleep,
            SinkType::Extract,
        ]
    }

    /// Psalm `TaintKind::TAINT_NAMES`: a docblock taint name expands to one
    /// or more sink kinds ("input"/"tainted" cover the whole input group).
    pub fn kinds_from_name(name: &str) -> Vec<SinkType> {
        match name {
            "input" | "tainted" => SinkType::all_input(),
            "input_except_sleep" => SinkType::all_input()
                .into_iter()
                .filter(|kind| !matches!(kind, SinkType::Sleep))
                .collect(),
            other => match SinkType::from_name(other) {
                Some(kind) => vec![kind],
                None => SinkType::all_input(),
            },
        }
    }

    /// Psalm's TAINT_NAMES (used in `@psalm-taint-*` docblock annotations).
    pub fn from_name(name: &str) -> Option<SinkType> {
        Some(match name {
            "input" => return None, // the whole input group — callers expand
            "callable" => SinkType::Callable,
            "unserialize" => SinkType::Unserialize,
            "include" => SinkType::Include,
            "eval" => SinkType::Eval,
            "ldap" => SinkType::Ldap,
            "sql" => SinkType::Sql,
            "html" => SinkType::Html,
            "has_quotes" => SinkType::HasQuotes,
            "shell" => SinkType::Shell,
            "ssrf" => SinkType::Ssrf,
            "file" => SinkType::FileSink,
            "cookie" => SinkType::Cookie,
            "header" => SinkType::Header,
            "xpath" => SinkType::Xpath,
            "sleep" => SinkType::Sleep,
            "extract" => SinkType::Extract,
            "user_secret" => SinkType::UserSecret,
            "system_secret" => SinkType::SystemSecret,
            other => SinkType::Custom(other.to_string()),
        })
    }

    /// The issue kind reported when tainted data reaches a sink of this type
    /// (Psalm's TaintedHtml / TaintedSql / … issue classes).
    pub fn issue_kind(&self) -> crate::issue::IssueKind {
        use crate::issue::IssueKind;
        match self {
            SinkType::Callable => IssueKind::TaintedCallable,
            SinkType::Unserialize => IssueKind::TaintedUnserialize,
            SinkType::Include => IssueKind::TaintedInclude,
            SinkType::Eval => IssueKind::TaintedEval,
            SinkType::Ldap => IssueKind::TaintedLdap,
            SinkType::Sql => IssueKind::TaintedSql,
            SinkType::Html => IssueKind::TaintedHtml,
            SinkType::HasQuotes => IssueKind::TaintedTextWithQuotes,
            SinkType::Shell => IssueKind::TaintedShell,
            SinkType::Ssrf => IssueKind::TaintedSSRF,
            SinkType::FileSink => IssueKind::TaintedFile,
            SinkType::Cookie => IssueKind::TaintedCookie,
            SinkType::Header => IssueKind::TaintedHeader,
            SinkType::Xpath => IssueKind::TaintedXpath,
            SinkType::Sleep => IssueKind::TaintedSleep,
            SinkType::Extract => IssueKind::TaintedExtract,
            SinkType::UserSecret => IssueKind::TaintedUserSecret,
            SinkType::SystemSecret => IssueKind::TaintedSystemSecret,
            SinkType::Unknown | SinkType::Custom(_) => IssueKind::TaintedInput,
        }
    }

    /// Human label used in issue messages ("Detected tainted html").
    pub fn label(&self) -> &str {
        match self {
            SinkType::Unknown => "input",
            SinkType::Callable => "callable",
            SinkType::Unserialize => "unserialize",
            SinkType::Include => "include",
            SinkType::Eval => "eval",
            SinkType::Ldap => "ldap",
            SinkType::Sql => "SQL",
            SinkType::Html => "HTML",
            SinkType::HasQuotes => "text with possible quotes",
            SinkType::Shell => "shell",
            SinkType::Ssrf => "SSRF",
            SinkType::FileSink => "file",
            SinkType::Cookie => "cookie",
            SinkType::Header => "header",
            SinkType::Xpath => "xpath",
            SinkType::Sleep => "sleep",
            SinkType::Extract => "extract",
            SinkType::UserSecret => "user secret",
            SinkType::SystemSecret => "system secret",
            SinkType::Custom(name) => name,
        }
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct DataFlowNode {
    pub id: DataFlowNodeId,
    pub kind: DataFlowNodeKind,
}

impl PartialEq for DataFlowNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataFlowNodeKind {
    Vertex {
        pos: Option<DataFlowNodePosition>,
        is_specialized: bool,
    },
    VariableUseSource {
        pos: DataFlowNodePosition,
        kind: VariableSourceKind,
        pure: bool,
        has_parent_nodes: bool,
        has_await_call: bool,
        has_awaitable: bool,
        from_loop_init: bool,
    },
    VariableUseSink {
        pos: DataFlowNodePosition,
    },
    ForLoopInit {
        var_id: VarId,
    },
    DataSource {
        pos: DataFlowNodePosition,
        target_id: String,
    },
    TaintSource {
        pos: Option<DataFlowNodePosition>,
        /// The sink kinds this source can taint (Psalm's source taints are
        /// expressed directly in the sink taxonomy).
        types: Vec<SinkType>,
    },
    TaintSink {
        /// Psalm sink nodes may have no location (e.g. `echo#1`, builtin
        /// param sinks); issues then fall back to the predecessor's position.
        pos: Option<DataFlowNodePosition>,
        types: Vec<SinkType>,
    },
}

impl Hash for DataFlowNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl DataFlowNode {
    pub fn get_for_method_argument(
        functionlike_id: &FunctionLikeIdentifier,
        argument_offset: usize,
        arg_location: Option<DataFlowNodePosition>,
        pos: Option<DataFlowNodePosition>,
    ) -> Self {
        let mut is_specialized = false;
        let mut id = DataFlowNodeId::FunctionLikeArg(*functionlike_id, argument_offset as u8);

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedFunctionLikeArg(
                *functionlike_id,
                argument_offset as u8,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: arg_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_property(property_id: (StrId, StrId)) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Property(property_id.0, property_id.1),
            kind: DataFlowNodeKind::Vertex {
                pos: None,
                is_specialized: false,
            },
        }
    }

    pub fn get_for_localized_property(
        property_id: (StrId, StrId),
        assignment_location: DataFlowNodePosition,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::SpecializedProperty(
                property_id.0,
                property_id.1,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_method_argument_out(
        functionlike_id: &FunctionLikeIdentifier,
        argument_offset: usize,
        arg_location: Option<DataFlowNodePosition>,
        pos: Option<DataFlowNodePosition>,
    ) -> Self {
        let mut is_specialized = false;
        let mut id = DataFlowNodeId::FunctionLikeOut(*functionlike_id, argument_offset as u8);

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedFunctionLikeOut(
                *functionlike_id,
                argument_offset as u8,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: arg_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_this_before_method(
        method_id: &MethodIdentifier,
        method_location: Option<DataFlowNodePosition>,
        pos: Option<DataFlowNodePosition>,
    ) -> Self {
        let mut is_specialized = false;
        let mut id = DataFlowNodeId::ThisBeforeMethod(*method_id);

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedThisBeforeMethod(
                *method_id,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: method_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_this_after_method(
        method_id: &MethodIdentifier,
        method_location: Option<DataFlowNodePosition>,
        pos: Option<DataFlowNodePosition>,
    ) -> Self {
        let mut is_specialized = false;
        let mut id = DataFlowNodeId::ThisAfterMethod(*method_id);

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedThisAfterMethod(
                *method_id,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: method_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_lvar(var_id: VarId, assignment_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Var(
                var_id,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_array_assignment(assignment_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::ArrayAssignment(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_return_expr(assignment_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Return(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_array_item(
        key_value: String,
        assignment_location: DataFlowNodePosition,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::ArrayItem(
                key_value,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_local_string(var_id: String, assignment_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::LocalString(
                var_id,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_instance_method_call(assignment_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::InstanceMethodCall(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_local_property_fetch(
        lhs_var_id: VarId,
        property_name: StrId,
        assignment_location: DataFlowNodePosition,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::PropertyFetch(
                lhs_var_id,
                property_name,
                assignment_location.file_path,
                assignment_location.start_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_narrowing(
        var_id: String,
        narrowed_symbol: &StrId,
        assignment_location: DataFlowNodePosition,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::VarNarrowedTo(
                var_id,
                *narrowed_symbol,
                assignment_location.file_path,
                assignment_location.start_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_type(type_name: &StrId, def_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Symbol(*type_name),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(def_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_call(
        functionlike_id: FunctionLikeIdentifier,
        assignment_location: DataFlowNodePosition,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::SpecializedCallTo(
                functionlike_id,
                assignment_location.file_path,
                assignment_location.start_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_composition(assignment_location: DataFlowNodePosition) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Composition(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_unlabelled_sink(assignment_location: DataFlowNodePosition) -> Self {
        Self {
            id: DataFlowNodeId::UnlabelledSink(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::VariableUseSink {
                pos: assignment_location,
            },
        }
    }

    pub fn get_for_variable_sink(label: VarId, assignment_location: DataFlowNodePosition) -> Self {
        Self {
            id: DataFlowNodeId::Var(
                label,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::VariableUseSink {
                pos: assignment_location,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_for_variable_source(
        kind: VariableSourceKind,
        label: VarId,
        assignment_location: DataFlowNodePosition,
        pure: bool,
        has_parent_nodes: bool,
        has_awaitable: bool,
        has_await_call: bool,
        from_loop_init: bool,
    ) -> Self {
        Self {
            id: DataFlowNodeId::Var(
                label,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::VariableUseSource {
                pos: assignment_location,
                kind,
                pure,
                has_awaitable,
                has_await_call,
                has_parent_nodes,
                from_loop_init,
            },
        }
    }

    pub fn get_for_method_return(
        functionlike_id: &FunctionLikeIdentifier,
        pos: Option<DataFlowNodePosition>,
        specialization_location: Option<DataFlowNodePosition>,
    ) -> Self {
        let mut is_specialized = false;
        let mut id = DataFlowNodeId::CallTo(*functionlike_id);

        if let Some(specialization_location) = specialization_location {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedCallTo(
                *functionlike_id,
                specialization_location.file_path,
                specialization_location.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos,
                is_specialized,
            },
        }
    }

    pub fn get_for_method_reference(
        functionlike_id: &FunctionLikeIdentifier,
        pos: Option<DataFlowNodePosition>,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::ReferenceTo(*functionlike_id),
            kind: DataFlowNodeKind::Vertex {
                pos,
                is_specialized: false,
            },
        }
    }

    #[inline]
    pub fn get_pos(&self) -> Option<DataFlowNodePosition> {
        match &self.kind {
            DataFlowNodeKind::Vertex { pos, .. } | DataFlowNodeKind::TaintSource { pos, .. } => {
                *pos
            }
            DataFlowNodeKind::TaintSink { pos, .. } => *pos,
            DataFlowNodeKind::VariableUseSource { pos, .. }
            | DataFlowNodeKind::DataSource { pos, .. }
            | DataFlowNodeKind::VariableUseSink { pos } => Some(*pos),
            DataFlowNodeKind::ForLoopInit { .. } => None,
        }
    }
}
