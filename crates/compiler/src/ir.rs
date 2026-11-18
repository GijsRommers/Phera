//! The compiler keeps an intermediate form to preserve PHP evaluation order before
//! committing to bytecode tables, compact instructions, and final jump offsets.
//! AST lowering emits owned names and writable paths. Register mapping moves named
//! locals ahead of temporaries. Bytecode lowering interns constants, names, types,
//! and paths, expands call and binding operations, and relocates branch targets.
//! The final bytecode optimization pass removes dead scalar temporaries and fuses
//! integer operands without moving observable PHP side effects across branches.

mod data;
pub use data::{
    Argument, ArgumentSource, Binary, ClassConstant, ClassDeclaration, ClassMethod, ClassProperty,
    CompiledUnit, Constant, Function, Parameter, PathStep, Place, PlaceScope, Target, Unary,
};
use phera_bytecode::Reg;

/// Compiler IR with owned names and paths before table interning.
#[derive(Debug, Clone)]
pub enum Instruction {
    /// Quiet receiver read for isset($this).
    ThisOrNull {
        destination: Reg,
    },
    /// Copy a lexical binding before allocating a closure object.
    Capture {
        source: Reg,
        kind: phera_bytecode::CaptureKind,
    },
    /// Instantiate an anonymous function with the preceding lexical captures.
    Closure {
        destination: Reg,
        function: usize,
        binding: phera_bytecode::ClosureBinding,
    },
    /// Validate the saved return before finally may intercept its outcome.
    CheckReturn {
        source: Reg,
    },
    /// Return a saved, already validated value or reference after finally.
    ReturnChecked {
        source: Reg,
    },
    /// Enter a shared finally subroutine and resume at the following instruction.
    FinallyCall {
        state: Reg,
        target: Target,
        reason: phera_bytecode::FinallyEntry,
    },
    /// Return from a finally subroutine or resume its pending exception.
    FinallyReturn {
        state: Reg,
    },
    /// Replace a pending continuation when control exits a finally body.
    FinallyDiscard {
        state: Reg,
    },
    /// Raise a Throwable expression; it has no normal successor.
    Throw {
        source: Reg,
    },
    /// Literal class/method operands need no intermediate PHP string registers.
    KnownStaticCall {
        class: String,
        method: String,
        binding: phera_bytecode::ClassBinding,
    },
    /// Allocate an evaluated class target before sending constructor arguments.
    NewDynamic {
        class: Reg,
    },
    /// Resolve a method name evaluated at runtime on an object.
    DynamicMethodCall {
        receiver: Reg,
        method: Reg,
    },
    /// Resolve a static-style call; forwarding retains the caller's late-bound class.
    StaticCall {
        class: Reg,
        method: Reg,
        binding: phera_bytecode::ClassBinding,
    },
    /// Test an evaluated class target without autoloading.
    DynamicInstanceOf {
        destination: Reg,
        source: Reg,
        class: Reg,
    },

    /// Fetch a constant using an evaluated class target.
    DynamicClassConstant {
        destination: Reg,
        class: Reg,
        name: Reg,
    },
    ClassConstant {
        destination: Reg,
        class: Reg,
        name: String,
    },
    /// Publish a request-local class declaration.
    DeclareClass(usize),
    /// Prepare a constructor using the same sends as a function call.
    New {
        class: String,
    },
    /// Resolve a literal method name on an evaluated receiver.
    MethodCall {
        receiver: Reg,
        method: String,
    },
    /// Resolve the class-name pseudo constant.
    ClassName {
        destination: Reg,
        class: String,
    },
    /// Fetch the receiver from the current method frame.
    This {
        destination: Reg,
    },
    /// Shallow-copy an object's property table.
    Clone {
        destination: Reg,
        source: Reg,
        with: Option<Reg>,
    },
    /// Test membership in a resolved class or interface.
    InstanceOf {
        destination: Reg,
        source: Reg,
        class: String,
    },
    /// Validate a local declaration; emits CheckType.
    Check {
        local: Reg,
        declaration_type: String,
    },
    /// Bind an evaluated global name into this scope; emits BindDynamicGlobal.
    DynamicGlobal {
        name: Reg,
    },
    /// Release a temporary owner to preserve copy-on-write behavior; emits Clear.
    Clear {
        register: Reg,
    },
    /// Skip a default expression when an argument was passed; emits ReceiveDefault.
    DefaultCheck {
        parameter: Reg,
        passed: Target,
    },
    /// Bind persistent function storage and skip initialization when present; emits BindStatic.
    StaticCheck {
        local: Reg,
        name: String,
        initialized: Target,
    },
    /// Initialize persistent function storage; emits InitStatic.
    StaticInit {
        local: Reg,
        name: String,
        source: Reg,
    },
    /// Resolve a named callee before evaluating arguments; emits InitCall.
    StartCall {
        name: String,
        fallback: Option<String>,
    },
    /// Resolve an evaluated callable; emits InitDynamicCall.
    StartDynamicCall {
        callee: Reg,
    },
    /// Send a value or writable argument in evaluation order; emits NameArgument and SendValue, SendLocal, SendPath, or SendSpread.
    Send {
        argument: Argument,
    },
    /// Invoke the prepared callee and save its result; emits Call.
    FinishCall {
        destination: Reg,
    },
    /// Capture the prepared first-class callable; emits MakeCallable.
    FinishCallable {
        destination: Reg,
    },
    /// Increment or decrement a writable place; emits IncDecLocal or IncDecPath.
    IncPlace {
        destination: Reg,
        place: Place,
        mode: phera_bytecode::IncDec,
    },
    /// Apply a binary operation through a place; emits AssignOp and Move, or AssignOpPath.
    Compound {
        destination: Reg,
        place: Place,
        operator: Binary,
        source: Reg,
    },

    /// Retain a writable reference cell; emits MakeReference.
    Reference {
        destination: Reg,
        place: Place,
    },
    /// Replace a binding with an existing cell; emits BindReference.
    Rebind {
        place: Place,
        source: Reg,
    },
    /// Execute PHP source in the current symbol table.
    Eval {
        destination: Reg,
        source: Reg,
    },
    /// Execute another file in this scope; emits Include.
    Include {
        destination: Reg,
        path: Reg,
        once: bool,
        required: bool,
    },
    /// Materialize a literal; emits LoadInt, LoadConst, or LoadNull.
    Load {
        destination: Reg,
        constant: usize,
    },
    /// Copy a dereferenced register value; emits Move.
    Move {
        destination: Reg,
        source: Reg,
    },
    /// Read a place with the selected warning policy; emits Move or Fetch.
    Read {
        destination: Reg,
        place: Place,
        mode: phera_bytecode::FetchMode,
    },
    /// Assign through an existing reference when present; emits Assign or AssignPath.
    Write {
        place: Place,
        source: Reg,
    },
    /// Prepare an indirect property destination before reading its local source.
    WriteFromLocal {
        place: Place,
        source: Reg,
        destination: Reg,
    },
    /// Alias two writable places; emits MakeReference, BindReference, and Clear.
    Bind {
        place: Place,
        source: Place,
    },
    /// Remove a binding or array element; emits UnsetPath.
    Unset(Place),
    /// Apply an already evaluated binary operation; emits Binary.
    Binary {
        destination: Reg,
        operator: Binary,
        left: Reg,
        right: Reg,
    },
    /// Apply a unary operation or explicit cast; emits Unary or Cast.
    Unary {
        destination: Reg,
        operator: Unary,
        source: Reg,
    },
    /// Write a PHP string conversion to output; emits Echo.
    Echo(Reg),
    /// Transfer control to an IR target; emits Jump with a relocated target.
    Jump(Target),
    /// Branch on PHP truthiness; emits JumpIf or JumpIfNot.
    Branch {
        value: Reg,
        target: Target,
        when: bool,
    },
    /// Branch when the value is null; emits JumpIfNull.
    BranchNull {
        value: Reg,
        target: Target,
    },
    /// Prepare and invoke an evaluated callee; emits InitDynamicCall, argument sends, and Call.
    Call {
        destination: Reg,
        name: Reg,
        fallback: Option<String>,
        arguments: Vec<Argument>,
    },
    /// Return a dereferenced value from this frame; emits Return.
    Return(Reg),
    /// Return an alias cell from this frame; emits ReturnReference.
    ReturnReference(Reg),
    ReturnMissing,
    /// Stop the request with a value or status; emits Exit.
    Exit(Reg),
    /// Enter or leave PHP error suppression; emits BeginSilence or EndSilence.
    Suppress(bool),
    /// Construct an empty array; emits NewArray.
    Array(Reg),
    /// Insert, append, reference-bind, or unpack an array entry; emits ArraySetReference, ArrayPushReference, or ArraySpread.
    ArrayPush {
        array: Reg,
        key: Option<Reg>,
        value: Reg,
        spread: bool,
    },
    /// Fetch a named PHP constant; emits FetchConstant.
    Constant {
        destination: Reg,
        name: String,
        fallback: Option<String>,
    },
    /// Define a request constant; emits DeclareConstant.
    Define {
        name: String,
        source: Reg,
    },
    /// Publish a conditional function declaration; emits DeclareFunction.
    Declare(usize),
    /// Bind a named global cell into a local; emits BindGlobal.
    Global {
        local: Reg,
        name: String,
    },
    /// Start snapshot or live reference traversal; emits IterStart or IterStartReference.
    IterInit {
        iterator: Reg,
        source: Reg,
        place: Option<Place>,
    },
    /// Advance traversal and branch on exhaustion; emits IterNext or IterNextKeyed.
    IterNext {
        iterator: Reg,
        key: Option<Reg>,
        value: Reg,
        end: Target,
    },
    /// Raise the unmatched-subject exception; emits Raise(UnhandledMatch).
    MatchError {
        subject: Reg,
    },
}
