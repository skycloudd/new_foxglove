use crate::ast::{self, Ast};
use crate::typed_ast::*;
use crate::Spanned;
use std::collections::HashMap;
use std::hash::Hash;

pub fn typecheck(ast: Spanned<Ast>) -> Result<Spanned<TypedAst>, String> {
    let mut checker = Typechecker::new();

    checker.typecheck_ast(ast)
}

struct Typechecker<'a> {
    engine: Engine,
    bindings: Scopes<&'a str, TypeId>,
}

impl<'a> Typechecker<'a> {
    fn new() -> Self {
        Self {
            engine: Engine::new(),
            bindings: Scopes::new(),
        }
    }

    fn typecheck_ast<'src: 'a>(
        &mut self,
        ast: Spanned<Ast<'src>>,
    ) -> Result<Spanned<TypedAst<'src>>, String> {
        self.bindings.push_scope();

        let statements = ast
            .0
            .statements
            .0
            .into_iter()
            .map(|stmt| self.typecheck_statement(stmt))
            .collect::<Result<Vec<_>, _>>()?;

        self.bindings.pop_scope();

        Ok((
            TypedAst {
                statements: (statements, ast.0.statements.1),
            },
            ast.1,
        ))
    }

    fn typecheck_statement<'src: 'a>(
        &mut self,
        stmt: Spanned<ast::Statement<'src>>,
    ) -> Result<Spanned<Statement<'src>>, String> {
        Ok((
            match stmt.0 {
                ast::Statement::Expr(expr) => {
                    let expr = self.typecheck_expr(expr)?;

                    Statement::Expr(expr)
                }
                ast::Statement::Block(statements) => {
                    self.bindings.push_scope();

                    let statements = statements
                        .0
                        .into_iter()
                        .map(|stmt| self.typecheck_statement(stmt))
                        .collect::<Result<Vec<_>, _>>()?;

                    self.bindings.pop_scope();

                    Statement::Block((statements, stmt.1))
                }
                ast::Statement::Let { name, value } => {
                    let value = self.typecheck_expr(*value)?;
                    let value_ty = self.engine.insert(type_to_typeinfo((value.0.ty, value.1)));

                    let var_ty = self.engine.insert((TypeInfo::Unknown, name.1));

                    self.engine.unify(value_ty, var_ty)?;

                    self.bindings.insert(name.0, var_ty);

                    Statement::Let {
                        name,
                        value: Box::new(value),
                    }
                }
                ast::Statement::Print(expr) => {
                    let expr = self.typecheck_expr(expr)?;

                    Statement::Print(expr)
                }
            },
            stmt.1,
        ))
    }

    fn typecheck_expr<'src>(
        &mut self,
        expr: Spanned<ast::Expr<'src>>,
    ) -> Result<Spanned<Expr<'src>>, String> {
        Ok((
            match expr.0 {
                ast::Expr::Var(name) => {
                    let ty = self.bindings.get(&name.0).ok_or("undefined_variable")?;

                    Expr {
                        expr: ExprKind::Var(name),
                        ty: self.engine.reconstruct(*ty)?.0,
                    }
                }
                ast::Expr::Literal(literal) => {
                    let literal = self.lower_literal(literal);

                    Expr {
                        expr: ExprKind::Literal(literal),
                        ty: literal.0.ty(),
                    }
                }
                ast::Expr::Prefix { op, expr } => {
                    let op = self.lower_prefix_operator(op);

                    let expr = self.typecheck_expr(*expr)?;
                    let expr_id = self.engine.insert(type_to_typeinfo((expr.0.ty, expr.1)));
                    let expr_ty = self.engine.reconstruct(expr_id)?;

                    let ty = expr_ty.0.get_prefix_type(op.0)?;

                    Expr {
                        expr: ExprKind::Prefix {
                            op,
                            expr: Box::new(expr),
                        },
                        ty,
                    }
                }
                ast::Expr::Binary { op, lhs, rhs } => {
                    let op = self.lower_binary_operator(op);

                    let lhs = self.typecheck_expr(*lhs)?;
                    let lhs_id = self.engine.insert(type_to_typeinfo((lhs.0.ty, lhs.1)));

                    let rhs = self.typecheck_expr(*rhs)?;
                    let rhs_id = self.engine.insert(type_to_typeinfo((rhs.0.ty, rhs.1)));

                    self.engine.unify(lhs_id, rhs_id)?;

                    let lhs_ty = self.engine.reconstruct(lhs_id)?;
                    let rhs_ty = self.engine.reconstruct(rhs_id)?;

                    let ty = lhs_ty.0.get_binary_type(&rhs_ty.0)?;

                    Expr {
                        expr: ExprKind::Binary {
                            op,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                        ty,
                    }
                }
            },
            expr.1,
        ))
    }

    fn lower_literal(&self, literal: Spanned<ast::Literal>) -> Spanned<Literal> {
        (
            match literal.0 {
                ast::Literal::Num(n) => Literal::Num(n),
            },
            literal.1,
        )
    }

    fn lower_prefix_operator(&self, op: Spanned<ast::PrefixOperator>) -> Spanned<PrefixOperator> {
        (
            match op.0 {
                ast::PrefixOperator::Negate => PrefixOperator::Negate,
            },
            op.1,
        )
    }

    fn lower_binary_operator(&self, op: Spanned<ast::BinaryOperator>) -> Spanned<BinaryOperator> {
        (
            match op.0 {
                ast::BinaryOperator::Add => BinaryOperator::Add,
                ast::BinaryOperator::Subtract => BinaryOperator::Subtract,
                ast::BinaryOperator::Multiply => BinaryOperator::Multiply,
                ast::BinaryOperator::Divide => BinaryOperator::Divide,
            },
            op.1,
        )
    }
}

struct Engine {
    id_counter: usize,
    vars: HashMap<TypeId, Spanned<TypeInfo>>,
}

impl Engine {
    fn new() -> Self {
        Self {
            id_counter: 0,
            vars: HashMap::new(),
        }
    }

    fn insert(&mut self, info: Spanned<TypeInfo>) -> TypeId {
        self.id_counter += 1;
        let id = self.id_counter;
        self.vars.insert(id, info);
        id
    }

    fn unify(&mut self, a: TypeId, b: TypeId) -> Result<(), String> {
        let var_a = self.vars[&a].clone();
        let var_b = self.vars[&b].clone();

        match (var_a.0, var_b.0) {
            (TypeInfo::Ref(a), _) => self.unify(a, b),
            (_, TypeInfo::Ref(b)) => self.unify(a, b),

            (TypeInfo::Unknown, _) => {
                self.vars.insert(a, (TypeInfo::Ref(b), var_b.1));
                Ok(())
            }
            (_, TypeInfo::Unknown) => {
                self.vars.insert(b, (TypeInfo::Ref(a), var_a.1));
                Ok(())
            }

            (TypeInfo::Num, TypeInfo::Num) => Ok(()),
        }
    }

    fn reconstruct(&mut self, id: TypeId) -> Result<Spanned<Type>, String> {
        let var = self.vars[&id].clone();

        Ok((
            match var.0 {
                TypeInfo::Unknown => return Err("cannot_infer_type".into()),
                TypeInfo::Ref(id) => self.reconstruct(id)?.0,
                TypeInfo::Num => Type::Num,
            },
            var.1,
        ))
    }
}

type TypeId = usize;

#[derive(Clone, Debug)]
pub enum TypeInfo {
    Unknown,
    Ref(TypeId),
    Num,
}

fn type_to_typeinfo(ty: Spanned<Type>) -> Spanned<TypeInfo> {
    (
        match ty.0 {
            Type::Num => TypeInfo::Num,
        },
        ty.1,
    )
}

#[derive(Clone, Debug)]
pub struct Scopes<K, V>(Vec<HashMap<K, V>>);

impl<K, V> Scopes<K, V> {
    pub fn new() -> Scopes<K, V> {
        Scopes(vec![HashMap::new()])
    }

    pub fn push_scope(&mut self) {
        self.0.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.0.pop();
    }

    pub fn insert(&mut self, k: K, v: V)
    where
        K: Eq + Hash,
    {
        self.0.last_mut().unwrap().insert(k, v);
    }

    pub fn get(&self, k: &K) -> Option<&V>
    where
        K: Eq + Hash,
    {
        for scope in self.0.iter().rev() {
            if let Some(v) = scope.get(k) {
                return Some(v);
            }
        }

        None
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V>
    where
        K: Eq + Hash,
    {
        for scope in self.0.iter_mut().rev() {
            if let Some(v) = scope.get_mut(k) {
                return Some(v);
            }
        }

        None
    }
}

impl Type {
    fn get_prefix_type(&self, op: PrefixOperator) -> Result<Type, String> {
        match self {
            Type::Num => match op {
                PrefixOperator::Negate => Ok(Type::Num),
            },
        }
    }

    fn get_binary_type(&self, rhs: &Type) -> Result<Type, String> {
        match (self, rhs) {
            (Type::Num, Type::Num) => Ok(Type::Num),
        }
    }
}

impl Literal {
    fn ty(&self) -> Type {
        match self {
            Literal::Num(_) => Type::Num,
        }
    }
}