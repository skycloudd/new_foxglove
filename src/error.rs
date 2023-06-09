use crate::typecheck::TypeInfo;
use crate::typed_ast::{BinOp, PrefixOp, Type};
use crate::{Span, Spanned};
use ariadne::{Color, Fmt};
use chumsky::error::RichReason;
use chumsky::prelude::Rich;

#[derive(Clone, PartialEq)]
pub enum Error {
    Typecheck(TypecheckError),
    ExpectedFound {
        span: Span,
        expected: Vec<String>,
        found: Option<String>,
    },
    Custom(Span, String),
    Many(Vec<Error>),
}

type Message = String;
type Spans = Vec<Spanned<(String, Color)>>;
type Notes = Vec<String>;

impl Error {
    pub fn make_report(&self) -> Vec<(Message, Spans, Notes)> {
        match self {
            Error::Typecheck(e) => vec![e.make_report()],
            Error::ExpectedFound {
                span,
                expected,
                found,
            } => vec![(
                format!(
                    "{}, expected {}",
                    if found.is_some() {
                        "Unexpected token in input"
                    } else {
                        "Unexpected end of input"
                    },
                    if expected.is_empty() {
                        "something else".to_owned()
                    } else {
                        expected
                            .iter()
                            .map(|expected| expected.fg(Color::Yellow).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ),
                vec![(
                    (
                        format!(
                            "Unexpected token {}",
                            format!("'{}'", found.as_ref().unwrap_or(&"eof".to_string()))
                                .fg(Color::Yellow)
                        ),
                        Color::Yellow,
                    ),
                    *span,
                )],
                vec![],
            )],
            Error::Custom(span, msg) => {
                vec![(
                    msg.to_string(),
                    vec![((String::new(), Color::Yellow), *span)],
                    vec![],
                )]
            }
            Error::Many(errors) => errors.iter().map(Error::make_report).flatten().collect(),
        }
    }

    pub fn code(&self) -> u32 {
        match self {
            Error::Typecheck(e) => match e {
                TypecheckError::UndefinedVariable { .. } => 2,
                TypecheckError::CannotInferType { .. } => 3,
                TypecheckError::TypeMismatch { .. } => 4,
                TypecheckError::CannotApplyUnaryOperator { .. } => 5,
                TypecheckError::CannotApplyBinaryOperator { .. } => 6,
            },
            Error::ExpectedFound { .. } => 1,
            Error::Custom(_, _) => 0,
            Error::Many(errs) => errs.iter().map(Error::code).max().unwrap_or(0),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum TypecheckError {
    UndefinedVariable {
        name: String,
        span: Span,
    },
    CannotInferType {
        span: Span,
    },
    TypeMismatch {
        span1: Span,
        span2: Span,
        ty1: TypeInfo,
        ty2: TypeInfo,
    },
    CannotApplyUnaryOperator {
        span: Span,
        op: PrefixOp,
        ty: Type,
    },
    CannotApplyBinaryOperator {
        span: Span,
        op: BinOp,
        ty1: Type,
        ty2: Type,
    },
}

impl TypecheckError {
    fn make_report(&self) -> (Message, Spans, Notes) {
        match self {
            TypecheckError::UndefinedVariable { name, span } => (
                format!("Undefined variable '{}'", name.fg(Color::Yellow)),
                vec![(
                    ("not found in this scope".to_string(), Color::Yellow),
                    *span,
                )],
                vec![],
            ),
            TypecheckError::CannotInferType { span } => (
                "Cannot infer type".to_string(),
                vec![(
                    (
                        "Cannot infer the type of this expression".to_string(),
                        Color::Yellow,
                    ),
                    *span,
                )],
                vec!["help: try adding a type annotation".to_string()],
            ),
            TypecheckError::TypeMismatch {
                span1,
                span2,
                ty1,
                ty2,
            } => (
                "Type mismatch".to_string(),
                vec![
                    ((format!("Type '{:?}' here", ty1), Color::Yellow), *span1),
                    ((format!("Type '{:?}' here", ty2), Color::Yellow), *span2),
                ],
                vec![],
            ),
            TypecheckError::CannotApplyUnaryOperator { span, op, ty } => (
                format!(
                    "Cannot apply operator '{}' to type '{}'",
                    op.fg(Color::Yellow),
                    format!("{:?}", ty).fg(Color::Yellow)
                ),
                vec![(
                    (
                        format!(
                            "Cannot apply this operator to type '{}'",
                            format!("{:?}", ty).fg(Color::Yellow)
                        ),
                        Color::Yellow,
                    ),
                    *span,
                )],
                vec![],
            ),
            TypecheckError::CannotApplyBinaryOperator { span, op, ty1, ty2 } => (
                format!(
                    "Cannot apply binary operator '{}' to types '{}' and '{}'",
                    op.fg(Color::Yellow),
                    format!("{:?}", ty1).fg(Color::Yellow),
                    format!("{:?}", ty2).fg(Color::Yellow)
                ),
                vec![(
                    (
                        format!(
                            "Cannot apply this operator to types '{}' and '{}'",
                            format!("{:?}", ty1).fg(Color::Yellow),
                            format!("{:?}", ty2).fg(Color::Yellow)
                        ),
                        Color::Yellow,
                    ),
                    *span,
                )],
                vec![],
            ),
        }
    }
}

impl From<TypecheckError> for Error {
    fn from(err: TypecheckError) -> Self {
        Self::Typecheck(err)
    }
}

impl From<Rich<'_, String>> for Error {
    fn from(value: Rich<'_, String>) -> Self {
        fn convert_reason(reason: RichReason<String>, span: Span) -> Error {
            match reason {
                RichReason::ExpectedFound { expected, found } => Error::ExpectedFound {
                    span: span,
                    expected: expected.iter().map(ToString::to_string).collect(),
                    found: found.map(|s| s.to_string()),
                },
                RichReason::Custom(reason) => Error::Custom(span, reason),
                RichReason::Many(reasons) => Error::Many(
                    reasons
                        .into_iter()
                        .map(|reason| convert_reason(reason, span))
                        .collect(),
                ),
            }
        }

        convert_reason(value.reason().clone(), *value.span())
    }
}
