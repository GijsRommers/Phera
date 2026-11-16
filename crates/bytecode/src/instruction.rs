//! The instruction set.

use crate::{ConstId, FunctionId, IterId, NameId, PathId, Reg, StaticId, SymbolId, Target, TypeId};

/// One step of the interpreter.
///
/// Conventions that hold for every instruction:
///
/// - `dst` is written, every other register is read.
/// - Reading a register sees through a PHP reference stored in it. Reading a
///   named local that was never assigned raises PHP's "Undefined variable"
///   warning and yields null. Temporaries are always assigned before use.
/// - Operands were evaluated by earlier instructions in PHP's order (left to
///   right), so an instruction never evaluates an expression itself.
/// - An instruction that can fail (a `TypeError`, a division by zero) throws;
///   it never leaves `dst` half written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Instruction {
    /// Does nothing. Left behind when the compiler removes an instruction.
    Nop,

    // ----- Loading values -------------------------------------------------
    /// `dst = constants[constant]`.
    LoadConst {
        dst: Reg,
        constant: ConstId,
    },
    LoadNull {
        dst: Reg,
    },
    LoadBool {
        dst: Reg,
        value: bool,
    },
    /// Small integers are by far the most common constants, so they travel
    /// inside the instruction.
    LoadInt {
        dst: Reg,
        value: i32,
    },
    /// `dst = src`, copying the value (not the reference, if `src` holds one).
    Move {
        dst: Reg,
        src: Reg,
    },
    /// Empties a temporary early. A temporary that still holds an array keeps
    /// that array shared, and the next write to it would copy it.
    Clear {
        reg: Reg,
    },

    // ----- Local variables ------------------------------------------------
    /// `$local = src`. Writes through a reference when the variable is one.
    Assign {
        local: Reg,
        src: Reg,
    },
    /// `$local op= src`. Kept apart from `Binary` + `Assign` so that
    /// `$s .= "x"` can append in place instead of copying the string.
    AssignOp {
        op: BinaryOp,
        local: Reg,
        src: Reg,
    },
    /// `++$local`, `$local--` and so on; `dst` receives the expression's value.
    IncDecLocal {
        mode: IncDec,
        dst: Reg,
        local: Reg,
    },
    UnsetLocal {
        local: Reg,
    },
    /// `dst = isset($local)`: assigned and not null.
    IssetLocal {
        dst: Reg,
        local: Reg,
    },

    // ----- Places reached through a path ----------------------------------
    // A path names a place inside a variable: `$a['x'][3]`, `$$name`,
    // `$a[]`. Writes walk it once, creating missing arrays and separating
    // shared ones (copy-on-write) at every level.
    /// `dst = <path>`.
    Fetch {
        dst: Reg,
        path: PathId,
        mode: FetchMode,
    },
    /// `<path> = src`.
    /// Append to a local array without looking up a one-step path table entry.
    /// References and scalar roots retain the ordinary PHP path-write behavior.
    AppendLocal {
        local: Reg,
        src: Reg,
    },
    AssignPath {
        path: PathId,
        src: Reg,
    },
    /// `<path> op= src`; `dst` receives the new value.
    AssignOpPath {
        op: BinaryOp,
        dst: Reg,
        path: PathId,
        src: Reg,
    },
    IncDecPath {
        mode: IncDec,
        dst: Reg,
        path: PathId,
    },
    UnsetPath {
        path: PathId,
    },
    IssetPath {
        dst: Reg,
        path: PathId,
    },

    // ----- References -----------------------------------------------------
    /// Turns the place into a PHP reference (if it is not one yet) and puts
    /// that reference itself, not its value, into `dst`. Only the four
    /// instructions below and the `Send`/`Array...Reference` ones read a
    /// register without seeing through the reference.
    MakeReference {
        dst: Reg,
        path: PathId,
    },
    /// `<path> = &...`: makes the place share the reference held in `src`.
    BindReference {
        path: PathId,
        src: Reg,
    },
    /// `global ${$name}`: resolves the evaluated name in dynamic scope and
    /// binds that variable to the request's global reference.
    BindDynamicGlobal {
        name: Reg,
    },
    /// `global $name;`: binds the local to the global variable of that name.
    BindGlobal {
        local: Reg,
        name: NameId,
    },
    /// `static $x = ...;`. If the static variable already has a value, binds
    /// the local to it and jumps to `initialized`. Otherwise falls through
    /// into the code that computes the initial value, which ends in
    /// `InitStatic`.
    BindStatic {
        local: Reg,
        variable: StaticId,
        initialized: Target,
    },
    /// Stores the initial value of a static variable and binds the local.
    InitStatic {
        local: Reg,
        variable: StaticId,
        src: Reg,
    },

    // ----- Operators ------------------------------------------------------
    /// Compare a register to an integer and branch when the result is false.
    /// The literal and target fit inline; no temporary boolean register is needed.
    BinaryIntBranch {
        op: BinaryOp,
        lhs: Reg,
        rhs: i32,
        target: Target,
    },
    /// A binary operation with a signed small integer. This avoids loading a
    /// literal register on every loop iteration; coercion matches `Binary`.
    BinaryInt {
        op: BinaryOp,
        dst: Reg,
        lhs: Reg,
        rhs: i32,
    },
    Binary {
        op: BinaryOp,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Unary {
        op: UnaryOp,
        dst: Reg,
        src: Reg,
    },
    Cast {
        to: CastKind,
        dst: Reg,
        src: Reg,
    },

    // ----- Building arrays ------------------------------------------------
    /// `dst = []`, with room for `capacity` elements.
    NewArray {
        dst: Reg,
        capacity: u16,
    },
    /// `array[] = src`.
    ArrayPush {
        array: Reg,
        src: Reg,
    },
    /// `array[key] = src`.
    ArraySet {
        array: Reg,
        key: Reg,
        src: Reg,
    },
    /// `[...$src]`: appends every element of `src`.
    ArraySpread {
        array: Reg,
        src: Reg,
    },
    /// `[&$x]`: `src` holds a reference from `MakeReference`.
    ArrayPushReference {
        array: Reg,
        src: Reg,
    },
    ArraySetReference {
        array: Reg,
        key: Reg,
        src: Reg,
    },

    // ----- Control flow ---------------------------------------------------
    Jump {
        target: Target,
    },
    /// Jumps when the value is truthy by PHP's rules.
    JumpIf {
        condition: Reg,
        target: Target,
    },
    JumpIfNot {
        condition: Reg,
        target: Target,
    },
    /// For `??` and `?->`.
    JumpIfNull {
        value: Reg,
        target: Target,
    },
    JumpIfNotNull {
        value: Reg,
        target: Target,
    },

    // ----- foreach --------------------------------------------------------
    /// Starts iterating over the value in `src`. Something that cannot be
    /// iterated raises PHP's warning and jumps to `end`.
    IterStart {
        iter: IterId,
        src: Reg,
        end: Target,
    },
    /// `foreach ($array as &$value)`: iterates over the place itself so the
    /// loop can modify it.
    IterStartReference {
        iter: IterId,
        path: PathId,
        end: Target,
    },
    /// Stores the next element in `value` (bound as a reference in a
    /// by-reference loop) or jumps to `end` when there is none.
    IterNext {
        iter: IterId,
        value: Reg,
        end: Target,
    },
    IterNextKeyed {
        iter: IterId,
        key: Reg,
        value: Reg,
        end: Target,
    },
    /// Releases the iterator. Emitted on every way out of the loop.
    IterEnd {
        iter: IterId,
    },

    // ----- Calls ----------------------------------------------------------
    // A call is spread over several instructions, as in PHP itself:
    // `InitCall` finds the function first, because only the callee knows
    // which parameters are by reference. Then one `Send` per argument, in
    // source order. Then `Call`. Calls nest (`f(g($x))`), so the frame keeps
    // a stack of calls that are being prepared.
    /// Starts a call to a function known by name.
    InitCall {
        function: SymbolId,
    },
    /// Starts a call to whatever callable the register holds.
    InitDynamicCall {
        callee: Reg,
    },
    /// Passes a computed value. If the parameter is by reference, PHP
    /// rejects this, except for function results, which pass with a notice.
    SendValue {
        src: Reg,
    },
    /// Passes a local variable: by reference if the parameter wants one,
    /// otherwise its value.
    SendLocal {
        local: Reg,
    },
    /// Passes a place such as `$a['x']`, by reference or by value likewise.
    SendPath {
        path: PathId,
    },
    /// `f(...$src)`.
    SendSpread {
        src: Reg,
    },
    /// The next `Send` is for the parameter with this name.
    NameArgument {
        name: NameId,
    },
    /// Runs the prepared call; `dst` receives the return value.
    Call {
        dst: Reg,
    },
    /// `f(...)`: instead of calling, puts the prepared callee into `dst` as a
    /// callable value.
    MakeCallable {
        dst: Reg,
    },

    // ----- Inside a function ----------------------------------------------
    /// At the start of a function, once per parameter that has a default. If
    /// the caller passed the argument, jumps to `passed`. Otherwise falls
    /// through into the code that computes the default and assigns it. This
    /// is how defaults can be any expression without a second evaluator.
    ReceiveDefault {
        param: Reg,
        passed: Target,
    },
    /// Checks and converts a local against a declared type, used where the
    /// call sequence cannot do it (a default that is not a constant).
    CheckType {
        local: Reg,
        ty: TypeId,
    },
    /// A by-reference function returns the reference itself. Ordinary `Return`
    /// copies through references, as every value operand does.
    ReturnReference {
        src: Reg,
    },
    /// Returns the value, after checking it against the declared return type.
    Return {
        src: Reg,
    },
    ReturnNull,

    // ----- Declarations and constants -------------------------------------
    /// Declares a function when execution reaches this point. Functions
    /// declared at the top level of a file are available before it runs and
    /// do not need this (see `CompiledUnit::hoisted`).
    DeclareFunction {
        function: FunctionId,
    },
    /// `dst = SOME_CONSTANT`.
    FetchConstant {
        dst: Reg,
        constant: SymbolId,
    },
    /// `const NAME = src;`
    DeclareConstant {
        name: NameId,
        src: Reg,
    },

    // ----- Everything else ------------------------------------------------
    Echo {
        src: Reg,
    },
    /// `include`, `require` and their `_once` forms. `dst` receives what the
    /// file returned.
    Include {
        kind: IncludeKind,
        dst: Reg,
        path: Reg,
    },
    /// `exit($src)`.
    Exit {
        src: Reg,
    },
    /// `@expr`: diagnostics are not shown until the matching `EndSilence`.
    BeginSilence,
    EndSilence,
    /// Throws one of the errors the language itself raises.
    Raise {
        error: RuntimeError,
        detail: Reg,
    },
}

// The interpreter copies one instruction per step. Keep them register-sized;
// move anything bigger into a table.
const _: () = assert!(std::mem::size_of::<Instruction>() <= 16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Concat,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    /// Logical `xor`. `&&` and `||` are jumps, because they short-circuit.
    LogicalXor,
    Equal,
    NotEqual,
    Identical,
    NotIdentical,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Spaceship,
    /// `??=` on a place; as a plain binary operator `??` is compiled to jumps.
    Coalesce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Negate,
    Not,
    BitwiseNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    /// `(unset)` was removed from PHP; `(void)` evaluates and discards.
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncDec {
    PreIncrement,
    PreDecrement,
    PostIncrement,
    PostDecrement,
}

/// How a read through a path treats what is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchMode {
    /// An ordinary read: missing variables, keys and offsets warn.
    Read,
    /// For `??`, `isset` and `empty`: missing things are null, silently.
    Quiet,
    /// For `[$a, $b] = $value`: a missing key warns, but destructuring
    /// something that is not an array yields nulls silently.
    Destructure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeKind {
    Include,
    IncludeOnce,
    Require,
    RequireOnce,
}

/// Errors thrown by the language rather than by a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeError {
    /// `match` found no arm; `detail` holds the unmatched value.
    UnhandledMatch,
}
