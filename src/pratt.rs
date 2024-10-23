//! Utilities for parsing expressions using
//! [Pratt parsing](https://en.wikipedia.org/wiki/Operator-precedence_parser#Pratt_parsing).
//!
//! *“Who am I? What is my purpose in life? Does it really, cosmically speaking, matter if I don’t get up and go to work?”*
//!
//! Pratt parsing is a powerful technique for defining and parsing operators of varying arity, precedence, and
//! associativity. Unlike [precedence climbing](https://en.wikipedia.org/wiki/Operator-precedence_parser), which
//! defines operator precedence by structurally composing parsers of decreasing precedence, Pratt parsing defines
//! precedence through a numerical
//! ['binding power'](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html#From-Precedence-to-Binding-Power)
//! that determines how strongly operators should bind to the operands around them.
//!
//! Pratt parsers are defined with the [`Parser::pratt`] method.
//!
//! When writing pratt parsers, it is necessary to first define an 'atomic' operand used by the parser for building up
//! expressions. In most languages, atoms are simple, self-delimiting patterns such as numeric and string literals,
//! identifiers, or parenthesised expressions. Once an atom has been defined, operators can also be defined that
//! operate upon said atoms.
//!
//! # Fold functions
//!
//! Because operators bind atoms together, pratt parsers require you to specify, for each operator, a function that
//! combines its operands together into a syntax tree. These functions are given as the last arguments of [`infix`],
//! [`prefix`], and [`postfix`].
//!
//! # Examples
//!
//! ```
//! use chumsky::prelude::*;
//! use chumsky::pratt::*;
//! use chumsky::extra;
//!
//! enum Expr {
//!     Add(Box<Self>, Box<Self>),
//!     Sub(Box<Self>, Box<Self>),
//!     Pow(Box<Self>, Box<Self>),
//!     Neg(Box<Self>),
//!     Factorial(Box<Self>),
//!     Deref(Box<Self>),
//!     Literal(i32),
//! }
//!
//! impl std::fmt::Display for Expr {
//!     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//!         match self {
//!             Self::Literal(literal) => write!(f, "{literal}"),
//!             Self::Factorial(left) => write!(f, "({left}!)"),
//!             Self::Deref(left) => write!(f, "(*{left})"),
//!             Self::Neg(right) => write!(f, "(-{right})"),
//!             Self::Add(left, right) => write!(f, "({left} + {right})"),
//!             Self::Sub(left, right) => write!(f, "({left} - {right})"),
//!             Self::Pow(left, right) => write!(f, "({left} ^ {right})"),
//!         }
//!     }
//! }
//!
//! let atom = text::int::<_, _, extra::Err<Simple<char>>>(10)
//!     .from_str()
//!     .unwrapped()
//!     .map(Expr::Literal)
//!     .padded();
//!
//! let op = |c| just(c).padded();
//!
//! let expr = atom.pratt((
//!     // We want factorial to happen before any negation, so we need its precedence to be higher than `Expr::Neg`.
//!     postfix(4, op('!'), |lhs, _, _| Expr::Factorial(Box::new(lhs))),
//!     // Just like in math, we want that if we write -x^2, our parser parses that as -(x^2), so we need it to have
//!     // exponents bind tighter than our prefix operators.
//!     infix(right(3), op('^'), |l, _, r, _| Expr::Pow(Box::new(l), Box::new(r))),
//!     // Notice the conflict with our `Expr::Sub`. This will still parse correctly. We want negation to happen before
//!     // `+` and `-`, so we set its precedence higher.
//!     prefix(2, op('-'), |_, rhs, _| Expr::Neg(Box::new(rhs))),
//!     prefix(2, op('*'), |_, rhs, _| Expr::Deref(Box::new(rhs))),
//!     // Our `-` and `+` bind the weakest, meaning that even if they occur first in an expression, they will be the
//!     // last executed.
//!     infix(left(1), op('+'), |l, _, r, _| Expr::Add(Box::new(l), Box::new(r))),
//!     infix(left(1), op('-'), |l, _, r, _| Expr::Sub(Box::new(l), Box::new(r))),
//! ))
//!     .map(|x| x.to_string());
//!
//! assert_eq!(
//!     expr.parse("*1 + -2! - -3^2").into_result(),
//!     Ok("(((*1) + (-(2!))) - (-(3 ^ 2)))".to_string()),
//! );
//! ```

use super::*;

macro_rules! op_check_and_emit {
    () => {
        #[doc(hidden)]
        fn do_parse_prefix_check<'parse>(
            &self,
            inp: &mut InputRef<'src, 'parse, I, E>,
            pre_expr: &input::Cursor<'src, 'parse, I>,
            f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Check, O>,
        ) -> PResult<Check, O> {
            self.do_parse_prefix::<Check>(inp, pre_expr, f)
        }
        #[doc(hidden)]
        fn do_parse_prefix_emit<'parse>(
            &self,
            inp: &mut InputRef<'src, 'parse, I, E>,
            pre_expr: &input::Cursor<'src, 'parse, I>,
            f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Emit, O>,
        ) -> PResult<Emit, O> {
            self.do_parse_prefix::<Emit>(inp, pre_expr, f)
        }
        #[doc(hidden)]
        fn do_parse_postfix_check<'parse>(
            &self,
            inp: &mut InputRef<'src, 'parse, I, E>,
            pre_expr: &input::Cursor<'src, 'parse, I>,
            lhs: (),
        ) -> Result<(), ()> {
            self.do_parse_postfix::<Check>(inp, pre_expr, lhs)
        }
        #[doc(hidden)]
        fn do_parse_postfix_emit<'parse>(
            &self,
            inp: &mut InputRef<'src, 'parse, I, E>,
            pre_expr: &input::Cursor<'src, 'parse, I>,
            lhs: O,
        ) -> Result<O, O> {
            self.do_parse_postfix::<Emit>(inp, pre_expr, lhs)
        }
        #[doc(hidden)]
        fn do_parse_infix_check<'parse>(
            &self,
            inp: &mut InputRef<'src, 'parse, I, E>,
            pre_expr: &input::Cursor<'src, 'parse, I>,
            lhs: (),
            f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Check, O>,
        ) -> Result<(), ()> {
            self.do_parse_infix::<Check>(inp, pre_expr, lhs, &f)
        }
        #[doc(hidden)]
        fn do_parse_infix_emit<'parse>(
            &self,
            inp: &mut InputRef<'src, 'parse, I, E>,
            pre_expr: &input::Cursor<'src, 'parse, I>,
            lhs: O,
            f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Emit, O>,
        ) -> Result<O, O> {
            self.do_parse_infix::<Emit>(inp, pre_expr, lhs, &f)
        }
    };
}

/// A type implemented by pratt parser operators.
pub trait Operator<'src, I, O, E>
where
    I: Input<'src>,
    E: ParserExtra<'src, I>,
{
    /// Box this operator, allowing it to be used via dynamic dispatch.
    fn boxed<'a>(self) -> Boxed<'src, 'a, I, O, E>
    where
        Self: Sized + MaybeSync + 'a,
    {
        Boxed(RefC::new(self))
    }

    #[inline(always)]
    #[doc(hidden)]
    fn is_infix(&self) -> bool {
        false
    }
    #[inline(always)]
    #[doc(hidden)]
    fn is_prefix(&self) -> bool {
        false
    }
    #[inline(always)]
    #[doc(hidden)]
    fn is_postfix(&self) -> bool {
        false
    }

    #[doc(hidden)]
    fn associativity(&self) -> Associativity;

    #[doc(hidden)]
    fn do_parse_prefix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: impl Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<M, O>,
    ) -> PResult<M, O>
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[doc(hidden)]
    fn do_parse_postfix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: M::Output<O>,
    ) -> Result<M::Output<O>, M::Output<O>>
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[doc(hidden)]
    fn do_parse_infix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: M::Output<O>,
        f: impl Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<M, O>,
    ) -> Result<M::Output<O>, M::Output<O>>
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[doc(hidden)]
    fn do_parse_prefix_check<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Check, O>,
    ) -> PResult<Check, O>;
    #[doc(hidden)]
    fn do_parse_prefix_emit<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Emit, O>,
    ) -> PResult<Emit, O>;
    #[doc(hidden)]
    fn do_parse_postfix_check<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: (),
    ) -> Result<(), ()>;
    #[doc(hidden)]
    fn do_parse_postfix_emit<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: O,
    ) -> Result<O, O>;
    #[doc(hidden)]
    fn do_parse_infix_check<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: (),
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Check, O>,
    ) -> Result<(), ()>;
    #[doc(hidden)]
    fn do_parse_infix_emit<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: O,
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Emit, O>,
    ) -> Result<O, O>;
}

/// A boxed pratt parser operator. See [`Operator`].
pub struct Boxed<'src, 'a, I, O, E>(RefC<sync::DynOperator<'src, 'a, I, O, E>>);

impl<'src, 'a, I, O, E> Clone for Boxed<'src, 'a, I, O, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<'src, 'a, I, O, E> Operator<'src, I, O, E> for Boxed<'src, 'a, I, O, E>
where
    I: Input<'src>,
    E: ParserExtra<'src, I>,
{
    #[inline(always)]
    fn is_infix(&self) -> bool {
        self.0.is_infix()
    }
    #[inline(always)]
    fn is_prefix(&self) -> bool {
        self.0.is_prefix()
    }
    #[inline(always)]
    fn is_postfix(&self) -> bool {
        self.0.is_postfix()
    }

    #[inline(always)]
    fn associativity(&self) -> Associativity {
        self.0.associativity()
    }

    #[inline(always)]
    fn do_parse_prefix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: impl Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<M, O>,
    ) -> PResult<M, O>
    where
        Self: Sized,
    {
        M::invoke_pratt_op_prefix(self, inp, pre_expr, f)
    }

    #[inline(always)]
    fn do_parse_postfix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: M::Output<O>,
    ) -> Result<M::Output<O>, M::Output<O>>
    where
        Self: Sized,
    {
        M::invoke_pratt_op_postfix(self, inp, pre_expr, lhs)
    }

    #[inline(always)]
    fn do_parse_infix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: M::Output<O>,
        f: impl Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<M, O>,
    ) -> Result<M::Output<O>, M::Output<O>>
    where
        Self: Sized,
    {
        M::invoke_pratt_op_infix(self, inp, pre_expr, lhs, f)
    }

    fn do_parse_prefix_check<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Check, O>,
    ) -> PResult<Check, O> {
        self.0.do_parse_prefix_check(inp, pre_expr, f)
    }
    fn do_parse_prefix_emit<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Emit, O>,
    ) -> PResult<Emit, O> {
        self.0.do_parse_prefix_emit(inp, pre_expr, f)
    }
    fn do_parse_postfix_check<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: (),
    ) -> Result<(), ()> {
        self.0.do_parse_postfix_check(inp, pre_expr, lhs)
    }
    fn do_parse_postfix_emit<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: O,
    ) -> Result<O, O> {
        self.0.do_parse_postfix_emit(inp, pre_expr, lhs)
    }
    fn do_parse_infix_check<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: (),
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Check, O>,
    ) -> Result<(), ()> {
        self.0.do_parse_infix_check(inp, pre_expr, lhs, &f)
    }
    fn do_parse_infix_emit<'parse>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: O,
        f: &dyn Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<Emit, O>,
    ) -> Result<O, O> {
        self.0.do_parse_infix_emit(inp, pre_expr, lhs, &f)
    }
}

/// Defines the [associativity](https://en.wikipedia.org/wiki/Associative_property) and binding power of an [`infix`]
/// operator (see [`left`] and [`right`]).
///
/// Higher binding powers should be used for higher precedence operators.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Associativity {
    /// Specifies that the operator should be left-associative, with the given binding power (see [`left`]).
    Left(u16),
    /// Specifies that the operator should be right-associative, with the given binding power (see [`right`]).
    Right(u16),
}

/// Specifies a left [`Associativity`] with the given binding power.
///
/// Left-associative operators are evaluated from the left-most terms, moving rightward. For example, the expression
/// `a + b + c + d` will be evaluated as `((a + b) + c) + d` because addition is conventionally left-associative.
pub fn left(binding_power: u16) -> Associativity {
    Associativity::Left(binding_power)
}

/// Specifies a right [`Associativity`] with the given binding power.
///
/// Right-associative operators are evaluated from the right-most terms, moving leftward. For example, the expression
/// `a ^ b ^ c ^ d` will be evaluated as `a ^ (b ^ (c ^ d))` because exponents are conventionally right-associative.
pub fn right(binding_power: u16) -> Associativity {
    Associativity::Right(binding_power)
}

impl Associativity {
    fn left_power(&self) -> u32 {
        match self {
            Self::Left(x) => *x as u32 * 2,
            Self::Right(x) => *x as u32 * 2 + 1,
        }
    }

    fn right_power(&self) -> u32 {
        match self {
            Self::Left(x) => *x as u32 * 2 + 1,
            Self::Right(x) => *x as u32 * 2,
        }
    }
}

/// See [`infix`].
pub struct Infix<'src, A, F, Atom, Op, I, E> {
    op_parser: A,
    fold: F,
    associativity: Associativity,
    #[allow(dead_code)]
    phantom: EmptyPhantom<&'src (Atom, Op, I, E)>,
}

impl<A: Copy, F: Copy, Atom, Op, I, E> Copy for Infix<'_, A, F, Atom, Op, I, E> {}
impl<A: Clone, F: Clone, Atom, Op, I, E> Clone for Infix<'_, A, F, Atom, Op, I, E> {
    fn clone(&self) -> Self {
        Self {
            op_parser: self.op_parser.clone(),
            fold: self.fold.clone(),
            associativity: self.associativity,
            phantom: EmptyPhantom::new(),
        }
    }
}

/// Specify a binary infix operator for a pratt parser with the given associativity, binding power, and
/// [fold function](crate::pratt#fold-functions).
///
/// Operators like addition, subtraction, multiplication, division, remainder, exponentiation, etc. are infix binary
/// operators in most languages.
///
/// See [`left`] and [`right`] for information about associativity.
///
/// The fold function (the last argument) tells the parser how to combine the operator and operands into a new
/// expression. It must have the following signature:
///
/// ```ignore
/// impl Fn(Atom, Op, Atom, &mut MapExtra<'a, '_, I, E>) -> O
/// ```
pub const fn infix<'src, A, F, Atom, Op, I, E>(
    associativity: Associativity,
    op_parser: A,
    fold: F,
) -> Infix<'src, A, F, Atom, Op, I, E>
where
    F: Fn(Atom, Op, Atom, &mut MapExtra<'src, '_, I, E>) -> Atom,
{
    Infix {
        op_parser,
        fold,
        associativity,
        phantom: EmptyPhantom::new(),
    }
}

impl<'src, I, O, E, A, F, Op> Operator<'src, I, O, E> for Infix<'src, A, F, O, Op, I, E>
where
    I: Input<'src>,
    E: ParserExtra<'src, I>,
    A: Parser<'src, I, Op, E>,
    F: Fn(O, Op, O, &mut MapExtra<'src, '_, I, E>) -> O,
{
    #[inline(always)]
    fn is_infix(&self) -> bool {
        true
    }

    #[inline(always)]
    fn associativity(&self) -> Associativity {
        self.associativity
    }

    #[inline]
    fn do_parse_infix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: M::Output<O>,
        f: impl Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<M, O>,
    ) -> Result<M::Output<O>, M::Output<O>>
    where
        Self: Sized,
    {
        match self.op_parser.go::<M>(inp) {
            Ok(op) => match f(inp, self.associativity().right_power()) {
                Ok(rhs) => Ok(M::combine(
                    M::combine(lhs, rhs, |lhs, rhs| (lhs, rhs)),
                    op,
                    |(lhs, rhs), op| (self.fold)(lhs, op, rhs, &mut MapExtra::new(pre_expr, inp)),
                )),
                Err(()) => Err(lhs),
            },
            Err(()) => Err(lhs),
        }
    }

    op_check_and_emit!();
}

/// See [`prefix`].
pub struct Prefix<'src, A, F, Atom, Op, I, E> {
    op_parser: A,
    fold: F,
    binding_power: u16,
    #[allow(dead_code)]
    phantom: EmptyPhantom<&'src (Atom, Op, I, E)>,
}

impl<A: Copy, F: Copy, Atom, Op, I, E> Copy for Prefix<'_, A, F, Atom, Op, I, E> {}
impl<A: Clone, F: Clone, Atom, Op, I, E> Clone for Prefix<'_, A, F, Atom, Op, I, E> {
    fn clone(&self) -> Self {
        Self {
            op_parser: self.op_parser.clone(),
            fold: self.fold.clone(),
            binding_power: self.binding_power,
            phantom: EmptyPhantom::new(),
        }
    }
}

/// Specify a unary prefix operator for a pratt parser with the given binding power and
/// [fold function](crate::pratt#fold-functions).
///
/// Operators like negation, not, dereferencing, etc. are prefix unary operators in most languages.
///
/// The fold function (the last argument) tells the parser how to combine the operator and operand into a new
/// expression. It must have the following signature:
///
/// ```ignore
/// impl Fn(Atom, Op, &mut MapExtra<'a, '_, I, E>) -> O
/// ```
pub const fn prefix<'src, A, F, Atom, Op, I, E>(
    binding_power: u16,
    op_parser: A,
    fold: F,
) -> Prefix<'src, A, F, Atom, Op, I, E>
where
    F: Fn(Op, Atom, &mut MapExtra<'src, '_, I, E>) -> Atom,
{
    Prefix {
        op_parser,
        fold,
        binding_power,
        phantom: EmptyPhantom::new(),
    }
}

impl<'src, I, O, E, A, F, Op> Operator<'src, I, O, E> for Prefix<'src, A, F, O, Op, I, E>
where
    I: Input<'src>,
    E: ParserExtra<'src, I>,
    A: Parser<'src, I, Op, E>,
    F: Fn(Op, O, &mut MapExtra<'src, '_, I, E>) -> O,
{
    #[inline(always)]
    fn is_prefix(&self) -> bool {
        true
    }

    #[inline(always)]
    fn associativity(&self) -> Associativity {
        Associativity::Left(self.binding_power)
    }

    #[inline]
    fn do_parse_prefix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        f: impl Fn(&mut InputRef<'src, 'parse, I, E>, u32) -> PResult<M, O>,
    ) -> PResult<M, O>
    where
        Self: Sized,
    {
        match self.op_parser.go::<M>(inp) {
            Ok(op) => match f(inp, self.associativity().left_power()) {
                Ok(rhs) => Ok(M::combine(op, rhs, |op, rhs| {
                    (self.fold)(op, rhs, &mut MapExtra::new(pre_expr, inp))
                })),
                Err(()) => Err(()),
            },
            Err(()) => Err(()),
        }
    }

    op_check_and_emit!();
}

/// See [`postfix`].
pub struct Postfix<'src, A, F, Atom, Op, I, E> {
    op_parser: A,
    fold: F,
    binding_power: u16,
    #[allow(dead_code)]
    phantom: EmptyPhantom<&'src (Atom, Op, I, E)>,
}

impl<A: Copy, F: Copy, Atom, Op, I, E> Copy for Postfix<'_, A, F, Atom, Op, I, E> {}
impl<A: Clone, F: Clone, Atom, Op, I, E> Clone for Postfix<'_, A, F, Atom, Op, I, E> {
    fn clone(&self) -> Self {
        Self {
            op_parser: self.op_parser.clone(),
            fold: self.fold.clone(),
            binding_power: self.binding_power,
            phantom: EmptyPhantom::new(),
        }
    }
}

/// Specify a unary postfix operator for a pratt parser with the given binding power and
/// [fold function](crate::pratt#fold-functions).
///
/// Operators like factorial, field access, etc. are postfix unary operators in most languages.
///
/// The fold function (the last argument) tells the parser how to combine the operator and operand into a new
/// expression. It must have the following signature:
///
/// ```ignore
/// impl Fn(Op, Atom, &mut MapExtra<'a, '_, I, E>) -> O
/// ```
pub const fn postfix<'src, A, F, Atom, Op, I, E>(
    binding_power: u16,
    op_parser: A,
    fold: F,
) -> Postfix<'src, A, F, Atom, Op, I, E>
where
    F: Fn(Atom, Op, &mut MapExtra<'src, '_, I, E>) -> Atom,
{
    Postfix {
        op_parser,
        fold,
        binding_power,
        phantom: EmptyPhantom::new(),
    }
}

impl<'src, I, O, E, A, F, Op> Operator<'src, I, O, E> for Postfix<'src, A, F, O, Op, I, E>
where
    I: Input<'src>,
    E: ParserExtra<'src, I>,
    A: Parser<'src, I, Op, E>,
    F: Fn(O, Op, &mut MapExtra<'src, '_, I, E>) -> O,
{
    #[inline(always)]
    fn is_postfix(&self) -> bool {
        true
    }

    #[inline(always)]
    fn associativity(&self) -> Associativity {
        Associativity::Left(self.binding_power)
    }

    #[inline]
    fn do_parse_postfix<'parse, M: Mode>(
        &self,
        inp: &mut InputRef<'src, 'parse, I, E>,
        pre_expr: &input::Cursor<'src, 'parse, I>,
        lhs: M::Output<O>,
    ) -> Result<M::Output<O>, M::Output<O>>
    where
        Self: Sized,
    {
        match self.op_parser.go::<M>(inp) {
            Ok(op) => Ok(M::combine(lhs, op, |lhs, op| {
                (self.fold)(lhs, op, &mut MapExtra::new(pre_expr, inp))
            })),
            Err(()) => Err(lhs),
        }
    }

    op_check_and_emit!();
}

/// See [`Parser::pratt`].
#[derive(Copy, Clone)]
pub struct Pratt<Atom, Ops> {
    pub(crate) atom: Atom,
    pub(crate) ops: Ops,
}

macro_rules! impl_pratt_for_tuple {
    () => {};
    ($head:ident $($X:ident)*) => {
        impl_pratt_for_tuple!($($X)*);
        impl_pratt_for_tuple!(~ $head $($X)*);
    };
    (~ $($X:ident)+) => {
        #[allow(unused_variables, non_snake_case)]
        impl<'a, Atom, $($X),*> Pratt<Atom, ($($X,)*)> {
            #[inline]
            fn pratt_go<M: Mode, I, O, E>(&self, inp: &mut InputRef<'a, '_, I, E>, min_power: u32) -> PResult<M, O>
            where
                I: Input<'a>,
                E: ParserExtra<'a, I>,
                Atom: Parser<'a, I, O, E>,
                $($X: Operator<'a, I, O, E>),*
            {
                let pre_expr = inp.save();
                let mut lhs = 'choice: {
                    let ($($X,)*) = &self.ops;

                    // Prefix unary operators
                    $(
                        if $X.is_prefix() {
                            match $X.do_parse_prefix::<M>(inp, pre_expr.cursor(), |inp, min_power| recursive::recurse(|| self.pratt_go::<M, _, _, _>(inp, min_power))) {
                                Ok(out) => break 'choice out,
                                Err(()) => inp.rewind(pre_expr.clone()),
                            }
                        }
                    )*

                    self.atom.go::<M>(inp)?
                };

                loop {
                    let ($($X,)*) = &self.ops;

                    let pre_op = inp.save();

                    // Postfix unary operators
                    $(
                        let assoc = $X.associativity();
                        if $X.is_postfix() && assoc.right_power() >= min_power {
                            match $X.do_parse_postfix::<M>(inp, pre_expr.cursor(), lhs) {
                                Ok(out) => {
                                    lhs = out;
                                    continue
                                },
                                Err(out) => {
                                    lhs = out;
                                    inp.rewind(pre_op.clone())
                                },
                            }
                        }
                    )*

                    // Infix binary operators
                    $(
                        let assoc = $X.associativity();
                        if $X.is_infix() && assoc.left_power() >= min_power {
                            match $X.do_parse_infix::<M>(inp, pre_expr.cursor(), lhs, |inp, min_power| recursive::recurse(|| self.pratt_go::<M, _, _, _>(inp, min_power))) {
                                Ok(out) => {
                                    lhs = out;
                                    continue
                                },
                                Err(out) => {
                                    lhs = out;
                                    inp.rewind(pre_op.clone())
                                },
                            }
                        }
                    )*

                    inp.rewind(pre_op);
                    break;
                }

                Ok(lhs)
            }
        }

        #[allow(unused_variables, non_snake_case)]
        impl<'a, I, O, E, Atom, $($X),*> ParserSealed<'a, I, O, E> for Pratt<Atom, ($($X,)*)>
        where
            I: Input<'a>,
            E: ParserExtra<'a, I>,
            Atom: Parser<'a, I, O, E>,
            $($X: Operator<'a, I, O, E>),*
        {
            fn go<M: Mode>(&self, inp: &mut InputRef<'a, '_, I, E>) -> PResult<M, O> {
                self.pratt_go::<M, _, _, _>(inp, 0)
            }

            go_extra!(O);
        }
    };
}

impl_pratt_for_tuple!(A_ B_ C_ D_ E_ F_ G_ H_ I_ J_ K_ L_ M_ N_ O_ P_ Q_ R_ S_ T_ U_ V_ W_ X_ Y_ Z_);

#[inline]
fn pratt_go_slice<'a, M: Mode, I, O, E, Atom, Op>(
    atom: &Atom,
    ops: &[Op],
    inp: &mut InputRef<'a, '_, I, E>,
    min_power: u32,
) -> PResult<M, O>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
    Atom: Parser<'a, I, O, E>,
    Op: Operator<'a, I, O, E>,
{
    let pre_expr = inp.save();
    let mut lhs = 'choice: {
        // Prefix unary operators
        for op in ops {
            if op.is_prefix() {
                match op.do_parse_prefix::<M>(inp, pre_expr.cursor(), |inp, min_power| {
                    recursive::recurse(|| {
                        pratt_go_slice::<M, _, _, _, _, _>(atom, ops, inp, min_power)
                    })
                }) {
                    Ok(out) => break 'choice out,
                    Err(()) => inp.rewind(pre_expr.clone()),
                }
            }
        }

        atom.go::<M>(inp)?
    };

    'luup: loop {
        let pre_op = inp.save();

        // Postfix unary operators
        for op in ops {
            let assoc = op.associativity();
            if op.is_postfix() && assoc.right_power() >= min_power {
                match op.do_parse_postfix::<M>(inp, pre_expr.cursor(), lhs) {
                    Ok(out) => {
                        lhs = out;
                        continue;
                    }
                    Err(out) => {
                        lhs = out;
                        inp.rewind(pre_op.clone())
                    }
                }
            }
        }

        // Infix binary operators
        for op in ops {
            let assoc = op.associativity();
            if op.is_infix() && assoc.left_power() >= min_power {
                match op.do_parse_infix::<M>(inp, pre_expr.cursor(), lhs, |inp, min_power| {
                    recursive::recurse(|| {
                        pratt_go_slice::<M, _, _, _, _, _>(atom, ops, inp, min_power)
                    })
                }) {
                    Ok(out) => {
                        lhs = out;
                        continue 'luup;
                    }
                    Err(out) => {
                        lhs = out;
                        inp.rewind(pre_op.clone())
                    }
                }
            }
        }

        inp.rewind(pre_op);
        break;
    }

    Ok(lhs)
}

impl<'a, I, O, E, Atom, Op> ParserSealed<'a, I, O, E> for Pratt<Atom, Vec<Op>>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
    Atom: Parser<'a, I, O, E>,
    Op: Operator<'a, I, O, E>,
{
    fn go<M: Mode>(&self, inp: &mut InputRef<'a, '_, I, E>) -> PResult<M, O> {
        pratt_go_slice::<M, _, _, _, _, _>(&self.atom, &self.ops, inp, 0)
    }

    go_extra!(O);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{extra::Err, prelude::*};

    fn factorial(x: i64) -> i64 {
        if x == 0 {
            1
        } else {
            x * factorial(x - 1)
        }
    }

    fn parser<'a>() -> impl Parser<'a, &'a str, i64> {
        let atom = text::int(10).padded().from_str::<i64>().unwrapped();

        atom.pratt((
            prefix(2, just('-'), |_, x: i64, _| -x),
            postfix(2, just('!'), |x, _, _| factorial(x)),
            infix(left(0), just('+'), |l, _, r, _| l + r),
            infix(left(0), just('-'), |l, _, r, _| l - r),
            infix(left(1), just('*'), |l, _, r, _| l * r),
            infix(left(1), just('/'), |l, _, r, _| l / r),
        ))
    }

    #[test]
    fn precedence() {
        assert_eq!(parser().parse("2 + 3 * 4").into_result(), Ok(14));
        assert_eq!(parser().parse("2 * 3 + 4").into_result(), Ok(10));
    }

    #[test]
    fn unary() {
        assert_eq!(parser().parse("-2").into_result(), Ok(-2));
        assert_eq!(parser().parse("4!").into_result(), Ok(24));
        assert_eq!(parser().parse("2 + 4!").into_result(), Ok(26));
        assert_eq!(parser().parse("-2 + 2").into_result(), Ok(0));
    }

    // TODO: Make this work
    // fn parser_dynamic<'a>() -> impl Parser<'a, &'a str, i64> {
    //     let atom = text::int(10).padded().from_str::<i64>().unwrapped();

    //     atom.pratt(vec![
    //         prefix(2, just('-'), |x: i64| -x).into(),
    //         postfix(2, just('!'), factorial).into(),
    //         infix(left(0), just('+'), |l, r| l + r).into(),
    //         infix(left(0), just('-'), |l, r| l - r).into(),
    //         infix(left(1), just('*'), |l, r| l * r).into(),
    //         infix(left(1), just('/'), |l, _, r| l / r).into(),
    //     ])
    // }

    enum Expr {
        Literal(i64),
        Not(Box<Expr>),
        Negate(Box<Expr>),
        Confusion(Box<Expr>),
        Factorial(Box<Expr>),
        Value(Box<Expr>),
        Add(Box<Expr>, Box<Expr>),
        Sub(Box<Expr>, Box<Expr>),
        Mul(Box<Expr>, Box<Expr>),
        Div(Box<Expr>, Box<Expr>),
    }

    impl std::fmt::Display for Expr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Literal(literal) => write!(f, "{literal}"),
                Self::Not(right) => write!(f, "(~{right})"),
                Self::Negate(right) => write!(f, "(-{right})"),
                Self::Confusion(right) => write!(f, "(§{right})"),
                Self::Factorial(right) => write!(f, "({right}!)"),
                Self::Value(right) => write!(f, "({right}$)"),
                Self::Add(left, right) => write!(f, "({left} + {right})"),
                Self::Sub(left, right) => write!(f, "({left} - {right})"),
                Self::Mul(left, right) => write!(f, "({left} * {right})"),
                Self::Div(left, right) => write!(f, "({left} / {right})"),
            }
        }
    }

    fn u(e: fn(Box<Expr>) -> Expr, r: Expr) -> Expr {
        e(Box::new(r))
    }
    fn i(e: fn(Box<Expr>, Box<Expr>) -> Expr, l: Expr, r: Expr) -> Expr {
        e(Box::new(l), Box::new(r))
    }

    fn expr_parser<'a>() -> impl Parser<'a, &'a str, String, Err<Simple<'a, char>>> {
        let atom = text::int(10).from_str().unwrapped().map(Expr::Literal);

        atom.pratt((
            infix(left(0), just('+'), |l, _, r, _| i(Expr::Add, l, r)),
            infix(left(0), just('-'), |l, _, r, _| i(Expr::Sub, l, r)),
            infix(right(1), just('*'), |l, _, r, _| i(Expr::Mul, l, r)),
            infix(right(1), just('/'), |l, _, r, _| i(Expr::Div, l, r)),
        ))
        .map(|x| x.to_string())
    }

    fn complete_parser<'a>() -> impl Parser<'a, &'a str, String, Err<Simple<'a, char>>> {
        expr_parser().then_ignore(end())
    }

    fn parse(input: &str) -> ParseResult<String, Simple<char>> {
        complete_parser().parse(input)
    }

    fn parse_partial(input: &str) -> ParseResult<String, Simple<char>> {
        expr_parser().lazy().parse(input)
    }

    fn unexpected<'a, C: Into<Option<MaybeRef<'a, char>>>, S: Into<SimpleSpan>>(
        c: C,
        span: S,
    ) -> Simple<'a, char> {
        <Simple<_> as Error<'_, &'_ str>>::expected_found(None, c.into(), span.into())
    }

    #[test]
    fn missing_first_expression() {
        assert_eq!(parse("").into_result(), Err(vec![unexpected(None, 0..0)]))
    }

    #[test]
    fn missing_later_expression() {
        assert_eq!(parse("1+").into_result(), Err(vec![unexpected(None, 2..2)]),);
    }

    #[test]
    fn invalid_first_expression() {
        assert_eq!(
            parse("?").into_result(),
            Err(vec![unexpected(Some('?'.into()), 0..1)]),
        );
    }

    #[test]
    fn invalid_later_expression() {
        assert_eq!(
            parse("1+?").into_result(),
            Err(vec![dbg!(unexpected(Some('?'.into()), 2..3))]),
        );
    }

    #[test]
    fn invalid_operator() {
        assert_eq!(
            parse("1?").into_result(),
            Err(vec![unexpected(Some('?'.into()), 1..2)]),
        );
    }

    #[test]
    fn invalid_operator_incomplete() {
        assert_eq!(parse_partial("1?").into_result(), Ok("1".to_string()),);
    }

    #[test]
    fn complex_nesting() {
        assert_eq!(
            parse_partial("1+2*3/4*5-6*7+8-9+10").into_result(),
            Ok("(((((1 + (2 * (3 / (4 * 5)))) - (6 * 7)) + 8) - 9) + 10)".to_string()),
        );
    }

    #[test]
    fn with_prefix_ops() {
        let atom = text::int::<_, _, Err<Simple<char>>>(10)
            .from_str()
            .unwrapped()
            .map(Expr::Literal);

        let parser = atom
            .pratt((
                // -- Prefix
                // Because we defined '*' and '/' as right associative operators,
                // in order to get these to function as expected, their strength
                // must be higher
                prefix(2, just('-'), |_, r, _| u(Expr::Negate, r)),
                prefix(2, just('~'), |_, r, _| u(Expr::Not, r)),
                // This is what happens when not
                prefix(1, just('§'), |_, r, _| u(Expr::Confusion, r)),
                // -- Infix
                infix(left(0), just('+'), |l, _, r, _| i(Expr::Add, l, r)),
                infix(left(0), just('-'), |l, _, r, _| i(Expr::Sub, l, r)),
                infix(right(1), just('*'), |l, _, r, _| i(Expr::Mul, l, r)),
                infix(right(1), just('/'), |l, _, r, _| i(Expr::Div, l, r)),
            ))
            .map(|x| x.to_string());

        assert_eq!(
            parser.parse("-1+§~2*3").into_result(),
            Ok("((-1) + (§((~2) * 3)))".to_string()),
        )
    }

    #[test]
    fn with_postfix_ops() {
        let atom = text::int::<_, _, Err<Simple<char>>>(10)
            .from_str()
            .unwrapped()
            .map(Expr::Literal);

        let parser = atom
            .pratt((
                // -- Postfix
                // Because we defined '*' and '/' as right associative operators,
                // in order to get these to function as expected, their strength
                // must be higher
                postfix(2, just('!'), |l, _, _| u(Expr::Factorial, l)),
                // This is what happens when not
                postfix(0, just('$'), |l, _, _| u(Expr::Value, l)),
                // -- Infix
                infix(left(1), just('+'), |l, _, r, _| i(Expr::Add, l, r)),
                infix(left(1), just('-'), |l, _, r, _| i(Expr::Sub, l, r)),
                infix(right(2), just('*'), |l, _, r, _| i(Expr::Mul, l, r)),
                infix(right(2), just('/'), |l, _, r, _| i(Expr::Div, l, r)),
            ))
            .map(|x| x.to_string());

        assert_eq!(
            parser.parse("1+2!$*3").into_result(),
            Ok("(((1 + (2!))$) * 3)".to_string()),
        )
    }

    #[test]
    fn with_pre_and_postfix_ops() {
        let atom = text::int::<_, _, Err<Simple<char>>>(10)
            .from_str()
            .unwrapped()
            .map(Expr::Literal);

        let parser = atom
            .pratt((
                // -- Prefix
                prefix(4, just('-'), |_, r, _| u(Expr::Negate, r)),
                prefix(4, just('~'), |_, r, _| u(Expr::Not, r)),
                prefix(1, just('§'), |_, r, _| u(Expr::Confusion, r)),
                // -- Postfix
                postfix(5, just('!'), |l, _, _| u(Expr::Factorial, l)),
                postfix(0, just('$'), |l, _, _| u(Expr::Value, l)),
                // -- Infix
                infix(left(1), just('+'), |l, _, r, _| i(Expr::Add, l, r)),
                infix(left(1), just('-'), |l, _, r, _| i(Expr::Sub, l, r)),
                infix(right(2), just('*'), |l, _, r, _| i(Expr::Mul, l, r)),
                infix(right(2), just('/'), |l, _, r, _| i(Expr::Div, l, r)),
            ))
            .map(|x| x.to_string());
        assert_eq!(
            parser.parse("§1+-~2!$*3").into_result(),
            Ok("(((§(1 + (-(~(2!)))))$) * 3)".to_string()),
        )
    }
}
