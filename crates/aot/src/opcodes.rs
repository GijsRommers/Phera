//! Explicit stable 16-byte wire codec for [`phera_bytecode::Instruction`].
//!
//! Every variant is mapped by a single macro table so that:
//! - Adding a new [`Instruction`] variant is a compile-time error until it gets
//!   a permanent opcode number.
//! - Opcode numbers are never silently renumbered when variants are reordered.
//! - [`schema_description`] deterministically captures every opcode number and
//!   operand signature for the `.phb` `runtime_abi_hash`.

use phera_bytecode::{
    BinaryOp, BuiltinFrame, BuiltinUnary, CallResult, CallbackArguments, CaptureKind, CastKind,
    ClassBinding, ClassId, ClosureBinding, ConstId, FetchMode, FinallyEntry, FunctionId, IncDec,
    IncludeKind, Instruction, IterId, MethodSiteId, NameId, PathId, Reg, RuntimeError, StaticId,
    SymbolId, Target, TypeId, TypeMask, UnaryOp,
};

use crate::error::ImageError;

/// Fixed wire size of one encoded instruction in bytes.
pub const ENCODED_INSTRUCTION_LEN: usize = 16;

/// Intermediate 16-byte wire frame: `{ op: u8, x: u8, a: u16, b: u16, c: u16, d: u32, e: u32 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WireOp {
    op: u8,
    x: u8,
    a: u16,
    b: u16,
    c: u16,
    d: u32,
    e: u32,
}

impl WireOp {
    const fn new(op: u8) -> Self {
        Self {
            op,
            x: 0,
            a: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
        }
    }

    fn to_bytes(self) -> [u8; ENCODED_INSTRUCTION_LEN] {
        let mut out = [0u8; ENCODED_INSTRUCTION_LEN];
        out[0] = self.op;
        out[1] = self.x;
        out[2..4].copy_from_slice(&self.a.to_le_bytes());
        out[4..6].copy_from_slice(&self.b.to_le_bytes());
        out[6..8].copy_from_slice(&self.c.to_le_bytes());
        out[8..12].copy_from_slice(&self.d.to_le_bytes());
        out[12..16].copy_from_slice(&self.e.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8; ENCODED_INSTRUCTION_LEN]) -> Self {
        Self {
            op: bytes[0],
            x: bytes[1],
            a: u16::from_le_bytes([bytes[2], bytes[3]]),
            b: u16::from_le_bytes([bytes[4], bytes[5]]),
            c: u16::from_le_bytes([bytes[6], bytes[7]]),
            d: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            e: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        }
    }
}

macro_rules! opcode_table {
    (
        $(
            $code:literal => $name:ident $pat:tt : $sig:literal {
                encode => $enc:expr,
                decode($w:ident) => $dec:expr,
            }
        ),* $(,)?
    ) => {
        /// Stable canonical schema string hashed into `runtime_abi_hash`.
        pub fn schema_description() -> &'static str {
            concat!(
                "phera-isa-v1;",
                $(stringify!($code), ":", stringify!($name), "(", $sig, ");",)*
            )
        }

        /// Encodes one [`Instruction`] into 16 deterministic little-endian bytes.
        pub fn encode_instruction(inst: Instruction) -> [u8; ENCODED_INSTRUCTION_LEN] {
            let wire = match inst {
                $(
                    Instruction::$name $pat => {
                        let mut w = WireOp::new($code);
                        let f: &mut dyn FnMut(&mut WireOp) = &mut $enc;
                        f(&mut w);
                        w
                    }
                )*
            };
            wire.to_bytes()
        }

        /// Decodes one 16-byte wire instruction back into an [`Instruction`].
        /// Rejects non-canonical encodings (non-zero unused bytes or flag bits).
        pub fn decode_instruction(bytes: &[u8; ENCODED_INSTRUCTION_LEN]) -> Result<Instruction, ImageError> {
            let wire = WireOp::from_bytes(bytes);
            let inst: Result<Instruction, ImageError> = match wire.op {
                $(
                    $code => {
                        let $w = wire;
                        $dec
                    }
                )*
                other => Err(ImageError::Corrupt(format!("unknown instruction opcode {other}"))),
            };
            let inst = inst?;
            if encode_instruction(inst) != *bytes {
                return Err(ImageError::Corrupt(format!(
                    "non-canonical encoding for instruction opcode {}",
                    wire.op
                )));
            }
            Ok(inst)
        }
    };
}

opcode_table! {
    0 => Nop {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::Nop)
        },
    },
    1 => GeneratorStart {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::GeneratorStart)
        },
    },
    2 => Yield { dst, key, value } : "dst:R,key:O<R>,value:O<R>" {
        encode => |w| {
            w.x = u8::from(key.is_some()) | (u8::from(value.is_some()) << 1);
            w.a = dst.0;
            w.b = key.map_or(0, |r| r.0);
            w.c = value.map_or(0, |r| r.0);
        },
        decode(w) => {
            if w.x & !0b11 != 0 {
                return Err(ImageError::Corrupt("invalid Yield presence flags".into()));
            }
            Ok(Instruction::Yield {
                dst: Reg(w.a),
                key: (w.x & 1 != 0).then_some(Reg(w.b)),
                value: (w.x & 2 != 0).then_some(Reg(w.c)),
            })
        },
    },
    3 => YieldFrom { dst, src } : "dst:R,src:R" {
        encode => |w| { w.a = dst.0; w.b = src.0; },
        decode(w) => Ok(Instruction::YieldFrom { dst: Reg(w.a), src: Reg(w.b) }),
    },
    4 => GeneratorReturn { src } : "src:R" {
        encode => |w| { w.a = src.0; },
        decode(w) => Ok(Instruction::GeneratorReturn { src: Reg(w.a) }),
    },
    5 => AttributeDiagnostics {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::AttributeDiagnostics)
        },
    },
    6 => CheckReturn { source } : "source:R" {
        encode => |w| { w.a = source.0; },
        decode(w) => Ok(Instruction::CheckReturn { source: Reg(w.a) }),
    },
    7 => ReturnChecked { source } : "source:R" {
        encode => |w| { w.a = source.0; },
        decode(w) => Ok(Instruction::ReturnChecked { source: Reg(w.a) }),
    },
    8 => FinallyCall { state, target, reason } : "state:R,target:T,reason:FE" {
        encode => |w| {
            w.x = encode_finally_entry(reason);
            w.a = state.0;
            w.d = target.0;
        },
        decode(w) => Ok(Instruction::FinallyCall {
            state: Reg(w.a),
            target: Target(w.d),
            reason: decode_finally_entry(w.x)?,
        }),
    },
    9 => FinallyReturn { state } : "state:R" {
        encode => |w| { w.a = state.0; },
        decode(w) => Ok(Instruction::FinallyReturn { state: Reg(w.a) }),
    },
    10 => FinallyDiscard { state } : "state:R" {
        encode => |w| { w.a = state.0; },
        decode(w) => Ok(Instruction::FinallyDiscard { state: Reg(w.a) }),
    },
    11 => Throw { source } : "source:R" {
        encode => |w| { w.a = source.0; },
        decode(w) => Ok(Instruction::Throw { source: Reg(w.a) }),
    },
    12 => FetchClassConstant { dst, class, name } : "dst:R,class:R,name:N" {
        encode => |w| { w.a = dst.0; w.b = class.0; w.d = name.0; },
        decode(w) => Ok(Instruction::FetchClassConstant { dst: Reg(w.a), class: Reg(w.b), name: NameId(w.d) }),
    },
    13 => FetchNamedClassConstant { dst, class, name } : "dst:R,class:S,name:N" {
        encode => |w| { w.a = dst.0; w.d = class.0; w.e = name.0; },
        decode(w) => Ok(Instruction::FetchNamedClassConstant { dst: Reg(w.a), class: SymbolId(w.d), name: NameId(w.e) }),
    },
    14 => DeclareClass { class } : "class:C" {
        encode => |w| { w.d = class.0; },
        decode(w) => Ok(Instruction::DeclareClass { class: ClassId(w.d) }),
    },
    15 => New { class } : "class:S" {
        encode => |w| { w.d = class.0; },
        decode(w) => Ok(Instruction::New { class: SymbolId(w.d) }),
    },
    16 => InitMethodCall { receiver, method } : "receiver:R,method:N" {
        encode => |w| { w.a = receiver.0; w.d = method.0; },
        decode(w) => Ok(Instruction::InitMethodCall { receiver: Reg(w.a), method: NameId(w.d) }),
    },
    17 => FetchClassName { dst, class } : "dst:R,class:S" {
        encode => |w| { w.a = dst.0; w.d = class.0; },
        decode(w) => Ok(Instruction::FetchClassName { dst: Reg(w.a), class: SymbolId(w.d) }),
    },
    18 => FetchThis { dst } : "dst:R" {
        encode => |w| { w.a = dst.0; },
        decode(w) => Ok(Instruction::FetchThis { dst: Reg(w.a) }),
    },
    19 => Clone { dst, src, with } : "dst:R,src:R,with:O<R>" {
        encode => |w| {
            w.x = u8::from(with.is_some());
            w.a = dst.0;
            w.b = src.0;
            w.c = with.map_or(0, |r| r.0);
        },
        decode(w) => {
            let has_with = decode_bool(w.x, "Clone::with")?;
            Ok(Instruction::Clone {
                dst: Reg(w.a),
                src: Reg(w.b),
                with: has_with.then_some(Reg(w.c)),
            })
        },
    },
    20 => InstanceOf { dst, src, class } : "dst:R,src:R,class:S" {
        encode => |w| { w.a = dst.0; w.b = src.0; w.d = class.0; },
        decode(w) => Ok(Instruction::InstanceOf { dst: Reg(w.a), src: Reg(w.b), class: SymbolId(w.d) }),
    },
    21 => InstanceOfBranch { src, class, target } : "src:R,class:S,target:T" {
        encode => |w| { w.a = src.0; w.d = class.0; w.e = target.0; },
        decode(w) => Ok(Instruction::InstanceOfBranch { src: Reg(w.a), class: SymbolId(w.d), target: Target(w.e) }),
    },
    22 => InstanceOfBranchIf { src, class, target } : "src:R,class:S,target:T" {
        encode => |w| { w.a = src.0; w.d = class.0; w.e = target.0; },
        decode(w) => Ok(Instruction::InstanceOfBranchIf { src: Reg(w.a), class: SymbolId(w.d), target: Target(w.e) }),
    },
    23 => LoadConst { dst, constant } : "dst:R,constant:K" {
        encode => |w| { w.a = dst.0; w.d = constant.0; },
        decode(w) => Ok(Instruction::LoadConst { dst: Reg(w.a), constant: ConstId(w.d) }),
    },
    24 => LoadNull { dst } : "dst:R" {
        encode => |w| { w.a = dst.0; },
        decode(w) => Ok(Instruction::LoadNull { dst: Reg(w.a) }),
    },
    25 => LoadBool { dst, value } : "dst:R,value:B" {
        encode => |w| { w.x = u8::from(value); w.a = dst.0; },
        decode(w) => Ok(Instruction::LoadBool { dst: Reg(w.a), value: decode_bool(w.x, "LoadBool::value")? }),
    },
    26 => LoadInt { dst, value } : "dst:R,value:i32" {
        encode => |w| { w.a = dst.0; w.d = value as u32; },
        decode(w) => Ok(Instruction::LoadInt { dst: Reg(w.a), value: w.d as i32 }),
    },
    27 => Move { dst, src } : "dst:R,src:R" {
        encode => |w| { w.a = dst.0; w.b = src.0; },
        decode(w) => Ok(Instruction::Move { dst: Reg(w.a), src: Reg(w.b) }),
    },
    28 => Clear { reg } : "reg:R" {
        encode => |w| { w.a = reg.0; },
        decode(w) => Ok(Instruction::Clear { reg: Reg(w.a) }),
    },
    29 => Assign { local, src } : "local:R,src:R" {
        encode => |w| { w.a = local.0; w.b = src.0; },
        decode(w) => Ok(Instruction::Assign { local: Reg(w.a), src: Reg(w.b) }),
    },
    30 => AssignOp { op, local, src } : "op:Bin,local:R,src:R" {
        encode => |w| { w.x = encode_binary_op(op); w.a = local.0; w.b = src.0; },
        decode(w) => Ok(Instruction::AssignOp { op: decode_binary_op(w.x)?, local: Reg(w.a), src: Reg(w.b) }),
    },
    31 => IncDecLocal { mode, dst, local } : "mode:ID,dst:R,local:R" {
        encode => |w| { w.x = encode_inc_dec(mode); w.a = dst.0; w.b = local.0; },
        decode(w) => Ok(Instruction::IncDecLocal { mode: decode_inc_dec(w.x)?, dst: Reg(w.a), local: Reg(w.b) }),
    },
    32 => UnsetLocal { local } : "local:R" {
        encode => |w| { w.a = local.0; },
        decode(w) => Ok(Instruction::UnsetLocal { local: Reg(w.a) }),
    },
    33 => IssetLocal { dst, local } : "dst:R,local:R" {
        encode => |w| { w.a = dst.0; w.b = local.0; },
        decode(w) => Ok(Instruction::IssetLocal { dst: Reg(w.a), local: Reg(w.b) }),
    },
    34 => Fetch { dst, path, mode } : "dst:R,path:P,mode:FM" {
        encode => |w| { w.x = encode_fetch_mode(mode); w.a = dst.0; w.d = path.0; },
        decode(w) => Ok(Instruction::Fetch { dst: Reg(w.a), path: PathId(w.d), mode: decode_fetch_mode(w.x)? }),
    },
    35 => AppendLocal { local, src } : "local:R,src:R" {
        encode => |w| { w.a = local.0; w.b = src.0; },
        decode(w) => Ok(Instruction::AppendLocal { local: Reg(w.a), src: Reg(w.b) }),
    },
    36 => AssignPath { path, src } : "path:P,src:R" {
        encode => |w| { w.a = src.0; w.d = path.0; },
        decode(w) => Ok(Instruction::AssignPath { path: PathId(w.d), src: Reg(w.a) }),
    },
    37 => AssignPathFromLocal { path, src, dst } : "path:P,src:R,dst:R" {
        encode => |w| { w.a = src.0; w.b = dst.0; w.d = path.0; },
        decode(w) => Ok(Instruction::AssignPathFromLocal { path: PathId(w.d), src: Reg(w.a), dst: Reg(w.b) }),
    },
    38 => AssignOpPath { op, dst, path, src } : "op:Bin,dst:R,path:P,src:R" {
        encode => |w| { w.x = encode_binary_op(op); w.a = dst.0; w.b = src.0; w.d = path.0; },
        decode(w) => Ok(Instruction::AssignOpPath { op: decode_binary_op(w.x)?, dst: Reg(w.a), path: PathId(w.d), src: Reg(w.b) }),
    },
    39 => IncDecPath { mode, dst, path } : "mode:ID,dst:R,path:P" {
        encode => |w| { w.x = encode_inc_dec(mode); w.a = dst.0; w.d = path.0; },
        decode(w) => Ok(Instruction::IncDecPath { mode: decode_inc_dec(w.x)?, dst: Reg(w.a), path: PathId(w.d) }),
    },
    40 => UnsetPath { path } : "path:P" {
        encode => |w| { w.d = path.0; },
        decode(w) => Ok(Instruction::UnsetPath { path: PathId(w.d) }),
    },
    41 => IssetPath { dst, path } : "dst:R,path:P" {
        encode => |w| { w.a = dst.0; w.d = path.0; },
        decode(w) => Ok(Instruction::IssetPath { dst: Reg(w.a), path: PathId(w.d) }),
    },
    42 => MakeReference { dst, path } : "dst:R,path:P" {
        encode => |w| { w.a = dst.0; w.d = path.0; },
        decode(w) => Ok(Instruction::MakeReference { dst: Reg(w.a), path: PathId(w.d) }),
    },
    43 => BindReference { path, src } : "path:P,src:R" {
        encode => |w| { w.a = src.0; w.d = path.0; },
        decode(w) => Ok(Instruction::BindReference { path: PathId(w.d), src: Reg(w.a) }),
    },
    44 => BindDynamicGlobal { name } : "name:R" {
        encode => |w| { w.a = name.0; },
        decode(w) => Ok(Instruction::BindDynamicGlobal { name: Reg(w.a) }),
    },
    45 => BindGlobal { local, name } : "local:R,name:N" {
        encode => |w| { w.a = local.0; w.d = name.0; },
        decode(w) => Ok(Instruction::BindGlobal { local: Reg(w.a), name: NameId(w.d) }),
    },
    46 => BindStatic { local, variable, initialized } : "local:R,variable:St,initialized:T" {
        encode => |w| { w.a = local.0; w.b = variable.0; w.d = initialized.0; },
        decode(w) => Ok(Instruction::BindStatic { local: Reg(w.a), variable: StaticId(w.b), initialized: Target(w.d) }),
    },
    47 => InitStatic { local, variable, src } : "local:R,variable:St,src:R" {
        encode => |w| { w.a = local.0; w.b = variable.0; w.c = src.0; },
        decode(w) => Ok(Instruction::InitStatic { local: Reg(w.a), variable: StaticId(w.b), src: Reg(w.c) }),
    },
    48 => BinaryIntBranch { op, lhs, rhs, target } : "op:Bin,lhs:R,rhs:i32,target:T" {
        encode => |w| { w.x = encode_binary_op(op); w.a = lhs.0; w.d = rhs as u32; w.e = target.0; },
        decode(w) => Ok(Instruction::BinaryIntBranch { op: decode_binary_op(w.x)?, lhs: Reg(w.a), rhs: w.d as i32, target: Target(w.e) }),
    },
    49 => BinaryIntBranchIf { op, lhs, rhs, target } : "op:Bin,lhs:R,rhs:i32,target:T" {
        encode => |w| { w.x = encode_binary_op(op); w.a = lhs.0; w.d = rhs as u32; w.e = target.0; },
        decode(w) => Ok(Instruction::BinaryIntBranchIf { op: decode_binary_op(w.x)?, lhs: Reg(w.a), rhs: w.d as i32, target: Target(w.e) }),
    },
    50 => BinaryInt { op, dst, lhs, rhs } : "op:Bin,dst:R,lhs:R,rhs:i32" {
        encode => |w| { w.x = encode_binary_op(op); w.a = dst.0; w.b = lhs.0; w.d = rhs as u32; },
        decode(w) => Ok(Instruction::BinaryInt { op: decode_binary_op(w.x)?, dst: Reg(w.a), lhs: Reg(w.b), rhs: w.d as i32 }),
    },
    51 => Binary { op, dst, lhs, rhs } : "op:Bin,dst:R,lhs:R,rhs:R" {
        encode => |w| { w.x = encode_binary_op(op); w.a = dst.0; w.b = lhs.0; w.c = rhs.0; },
        decode(w) => Ok(Instruction::Binary { op: decode_binary_op(w.x)?, dst: Reg(w.a), lhs: Reg(w.b), rhs: Reg(w.c) }),
    },
    52 => BinaryBranch { op, lhs, rhs, target } : "op:Bin,lhs:R,rhs:R,target:T" {
        encode => |w| { w.x = encode_binary_op(op); w.a = lhs.0; w.b = rhs.0; w.d = target.0; },
        decode(w) => Ok(Instruction::BinaryBranch { op: decode_binary_op(w.x)?, lhs: Reg(w.a), rhs: Reg(w.b), target: Target(w.d) }),
    },
    53 => BinaryBranchIf { op, lhs, rhs, target } : "op:Bin,lhs:R,rhs:R,target:T" {
        encode => |w| { w.x = encode_binary_op(op); w.a = lhs.0; w.b = rhs.0; w.d = target.0; },
        decode(w) => Ok(Instruction::BinaryBranchIf { op: decode_binary_op(w.x)?, lhs: Reg(w.a), rhs: Reg(w.b), target: Target(w.d) }),
    },
    54 => Unary { op, dst, src } : "op:Un,dst:R,src:R" {
        encode => |w| { w.x = encode_unary_op(op); w.a = dst.0; w.b = src.0; },
        decode(w) => Ok(Instruction::Unary { op: decode_unary_op(w.x)?, dst: Reg(w.a), src: Reg(w.b) }),
    },
    55 => Cast { to, dst, src } : "to:Cast,dst:R,src:R" {
        encode => |w| { w.x = encode_cast_kind(to); w.a = dst.0; w.b = src.0; },
        decode(w) => Ok(Instruction::Cast { to: decode_cast_kind(w.x)?, dst: Reg(w.a), src: Reg(w.b) }),
    },
    56 => NewArray { dst, capacity } : "dst:R,capacity:u16" {
        encode => |w| { w.a = dst.0; w.b = capacity; },
        decode(w) => Ok(Instruction::NewArray { dst: Reg(w.a), capacity: w.b }),
    },
    57 => ArrayPush { array, src } : "array:R,src:R" {
        encode => |w| { w.a = array.0; w.b = src.0; },
        decode(w) => Ok(Instruction::ArrayPush { array: Reg(w.a), src: Reg(w.b) }),
    },
    58 => ArraySet { array, key, src } : "array:R,key:R,src:R" {
        encode => |w| { w.a = array.0; w.b = key.0; w.c = src.0; },
        decode(w) => Ok(Instruction::ArraySet { array: Reg(w.a), key: Reg(w.b), src: Reg(w.c) }),
    },
    59 => ArraySpread { array, src } : "array:R,src:R" {
        encode => |w| { w.a = array.0; w.b = src.0; },
        decode(w) => Ok(Instruction::ArraySpread { array: Reg(w.a), src: Reg(w.b) }),
    },
    60 => ArrayPushReference { array, src } : "array:R,src:R" {
        encode => |w| { w.a = array.0; w.b = src.0; },
        decode(w) => Ok(Instruction::ArrayPushReference { array: Reg(w.a), src: Reg(w.b) }),
    },
    61 => ArraySetReference { array, key, src } : "array:R,key:R,src:R" {
        encode => |w| { w.a = array.0; w.b = key.0; w.c = src.0; },
        decode(w) => Ok(Instruction::ArraySetReference { array: Reg(w.a), key: Reg(w.b), src: Reg(w.c) }),
    },
    62 => Jump { target } : "target:T" {
        encode => |w| { w.d = target.0; },
        decode(w) => Ok(Instruction::Jump { target: Target(w.d) }),
    },
    63 => JumpIf { condition, target } : "condition:R,target:T" {
        encode => |w| { w.a = condition.0; w.d = target.0; },
        decode(w) => Ok(Instruction::JumpIf { condition: Reg(w.a), target: Target(w.d) }),
    },
    64 => JumpIfNot { condition, target } : "condition:R,target:T" {
        encode => |w| { w.a = condition.0; w.d = target.0; },
        decode(w) => Ok(Instruction::JumpIfNot { condition: Reg(w.a), target: Target(w.d) }),
    },
    65 => JumpIfNull { value, target } : "value:R,target:T" {
        encode => |w| { w.a = value.0; w.d = target.0; },
        decode(w) => Ok(Instruction::JumpIfNull { value: Reg(w.a), target: Target(w.d) }),
    },
    66 => JumpIfNotNull { value, target } : "value:R,target:T" {
        encode => |w| { w.a = value.0; w.d = target.0; },
        decode(w) => Ok(Instruction::JumpIfNotNull { value: Reg(w.a), target: Target(w.d) }),
    },
    67 => IterStart { iter, src, end } : "iter:It,src:R,end:T" {
        encode => |w| { w.a = iter.0; w.b = src.0; w.d = end.0; },
        decode(w) => Ok(Instruction::IterStart { iter: IterId(w.a), src: Reg(w.b), end: Target(w.d) }),
    },
    68 => IterStartReference { subject, iter, path, end } : "subject:R,iter:It,path:P,end:T" {
        encode => |w| { w.a = subject.0; w.b = iter.0; w.d = path.0; w.e = end.0; },
        decode(w) => Ok(Instruction::IterStartReference { subject: Reg(w.a), iter: IterId(w.b), path: PathId(w.d), end: Target(w.e) }),
    },
    69 => IterNext { iter, value, end } : "iter:It,value:R,end:T" {
        encode => |w| { w.a = iter.0; w.b = value.0; w.d = end.0; },
        decode(w) => Ok(Instruction::IterNext { iter: IterId(w.a), value: Reg(w.b), end: Target(w.d) }),
    },
    70 => IterNextKeyed { iter, key, value, end } : "iter:It,key:R,value:R,end:T" {
        encode => |w| { w.a = iter.0; w.b = key.0; w.c = value.0; w.d = end.0; },
        decode(w) => Ok(Instruction::IterNextKeyed { iter: IterId(w.a), key: Reg(w.b), value: Reg(w.c), end: Target(w.d) }),
    },
    71 => IterEnd { iter } : "iter:It" {
        encode => |w| { w.a = iter.0; },
        decode(w) => Ok(Instruction::IterEnd { iter: IterId(w.a) }),
    },
    72 => InitCall { function } : "function:S" {
        encode => |w| { w.d = function.0; },
        decode(w) => Ok(Instruction::InitCall { function: SymbolId(w.d) }),
    },
    73 => InitDynamicCall { callee } : "callee:R" {
        encode => |w| { w.a = callee.0; },
        decode(w) => Ok(Instruction::InitDynamicCall { callee: Reg(w.a) }),
    },
    74 => SendValue { src, of_call } : "src:R,of_call:B" {
        encode => |w| { w.x = u8::from(of_call); w.a = src.0; },
        decode(w) => Ok(Instruction::SendValue { src: Reg(w.a), of_call: decode_bool(w.x, "SendValue::of_call")? }),
    },
    75 => SendLocal { local } : "local:R" {
        encode => |w| { w.a = local.0; },
        decode(w) => Ok(Instruction::SendLocal { local: Reg(w.a) }),
    },
    76 => SendPath { path } : "path:P" {
        encode => |w| { w.d = path.0; },
        decode(w) => Ok(Instruction::SendPath { path: PathId(w.d) }),
    },
    77 => SendSpread { src } : "src:R" {
        encode => |w| { w.a = src.0; },
        decode(w) => Ok(Instruction::SendSpread { src: Reg(w.a) }),
    },
    78 => NameArgument { name } : "name:N" {
        encode => |w| { w.d = name.0; },
        decode(w) => Ok(Instruction::NameArgument { name: NameId(w.d) }),
    },
    79 => Call { dst, result } : "dst:R,result:CR" {
        encode => |w| { w.x = encode_call_result(result); w.a = dst.0; },
        decode(w) => Ok(Instruction::Call { dst: Reg(w.a), result: decode_call_result(w.x)? }),
    },
    80 => AssignTake { local, src } : "local:R,src:R" {
        encode => |w| { w.a = local.0; w.b = src.0; },
        decode(w) => Ok(Instruction::AssignTake { local: Reg(w.a), src: Reg(w.b) }),
    },
    81 => SendTake { src, of_call } : "src:R,of_call:B" {
        encode => |w| { w.x = u8::from(of_call); w.a = src.0; },
        decode(w) => Ok(Instruction::SendTake { src: Reg(w.a), of_call: decode_bool(w.x, "SendTake::of_call")? }),
    },
    82 => AssignConst { local, constant } : "local:R,constant:K" {
        encode => |w| { w.a = local.0; w.d = constant.0; },
        decode(w) => Ok(Instruction::AssignConst { local: Reg(w.a), constant: ConstId(w.d) }),
    },
    83 => SendConst { constant } : "constant:K" {
        encode => |w| { w.d = constant.0; },
        decode(w) => Ok(Instruction::SendConst { constant: ConstId(w.d) }),
    },
    84 => CallMethod { site, dst, result } : "site:MS,dst:R,result:CR" {
        encode => |w| { w.x = encode_call_result(result); w.a = dst.0; w.d = site.0; },
        decode(w) => Ok(Instruction::CallMethod { site: MethodSiteId(w.d), dst: Reg(w.a), result: decode_call_result(w.x)? }),
    },
    85 => CallDirect { function, args, count, dst, result } : "function:F,args:R,count:u16,dst:R,result:CR" {
        encode => |w| { w.x = encode_call_result(result); w.a = args.0; w.b = count; w.c = dst.0; w.d = function.0; },
        decode(w) => Ok(Instruction::CallDirect { function: FunctionId(w.d), args: Reg(w.a), count: w.b, dst: Reg(w.c), result: decode_call_result(w.x)? }),
    },
    86 => MakeCallable { dst } : "dst:R" {
        encode => |w| { w.a = dst.0; },
        decode(w) => Ok(Instruction::MakeCallable { dst: Reg(w.a) }),
    },
    87 => ReceiveDefault { param, passed } : "param:R,passed:T" {
        encode => |w| { w.a = param.0; w.d = passed.0; },
        decode(w) => Ok(Instruction::ReceiveDefault { param: Reg(w.a), passed: Target(w.d) }),
    },
    88 => CheckType { local, ty } : "local:R,ty:Ty" {
        encode => |w| { w.a = local.0; w.d = ty.0; },
        decode(w) => Ok(Instruction::CheckType { local: Reg(w.a), ty: TypeId(w.d) }),
    },
    89 => ReturnReference { src } : "src:R" {
        encode => |w| { w.a = src.0; },
        decode(w) => Ok(Instruction::ReturnReference { src: Reg(w.a) }),
    },
    90 => Return { src } : "src:R" {
        encode => |w| { w.a = src.0; },
        decode(w) => Ok(Instruction::Return { src: Reg(w.a) }),
    },
    91 => ReturnNull {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::ReturnNull)
        },
    },
    92 => ReturnMissing {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::ReturnMissing)
        },
    },
    93 => DeclareFunction { function } : "function:F" {
        encode => |w| { w.d = function.0; },
        decode(w) => Ok(Instruction::DeclareFunction { function: FunctionId(w.d) }),
    },
    94 => FetchConstant { dst, constant } : "dst:R,constant:S" {
        encode => |w| { w.a = dst.0; w.d = constant.0; },
        decode(w) => Ok(Instruction::FetchConstant { dst: Reg(w.a), constant: SymbolId(w.d) }),
    },
    95 => DeclareConstant { name, src } : "name:N,src:R" {
        encode => |w| { w.a = src.0; w.d = name.0; },
        decode(w) => Ok(Instruction::DeclareConstant { name: NameId(w.d), src: Reg(w.a) }),
    },
    96 => Echo { src } : "src:R" {
        encode => |w| { w.a = src.0; },
        decode(w) => Ok(Instruction::Echo { src: Reg(w.a) }),
    },
    97 => Include { kind, dst, path } : "kind:Inc,dst:R,path:R" {
        encode => |w| { w.x = encode_include_kind(kind); w.a = dst.0; w.b = path.0; },
        decode(w) => Ok(Instruction::Include { kind: decode_include_kind(w.x)?, dst: Reg(w.a), path: Reg(w.b) }),
    },
    98 => Exit { src } : "src:R" {
        encode => |w| { w.a = src.0; },
        decode(w) => Ok(Instruction::Exit { src: Reg(w.a) }),
    },
    99 => BeginSilence {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::BeginSilence)
        },
    },
    100 => EndSilence {} : "" {
        encode => |_| {},
        decode(w) => {
            ensure_zero_rest(w, 0, 0, 0, 0, 0, 0)?;
            Ok(Instruction::EndSilence)
        },
    },
    101 => Raise { error, detail } : "error:RE,detail:R" {
        encode => |w| { w.x = encode_runtime_error(error); w.a = detail.0; },
        decode(w) => Ok(Instruction::Raise { error: decode_runtime_error(w.x)?, detail: Reg(w.a) }),
    },
    102 => NewDynamic { class } : "class:R" {
        encode => |w| { w.a = class.0; },
        decode(w) => Ok(Instruction::NewDynamic { class: Reg(w.a) }),
    },
    103 => DynamicMethodCall { receiver, method } : "receiver:R,method:R" {
        encode => |w| { w.a = receiver.0; w.b = method.0; },
        decode(w) => Ok(Instruction::DynamicMethodCall { receiver: Reg(w.a), method: Reg(w.b) }),
    },
    104 => StaticCall { class, method, binding } : "class:R,method:R,binding:CB" {
        encode => |w| { w.x = encode_class_binding(binding); w.a = class.0; w.b = method.0; },
        decode(w) => Ok(Instruction::StaticCall { class: Reg(w.a), method: Reg(w.b), binding: decode_class_binding(w.x)? }),
    },
    105 => DynamicInstanceOf { dst, source, class } : "dst:R,source:R,class:R" {
        encode => |w| { w.a = dst.0; w.b = source.0; w.c = class.0; },
        decode(w) => Ok(Instruction::DynamicInstanceOf { dst: Reg(w.a), source: Reg(w.b), class: Reg(w.c) }),
    },
    106 => DynamicInstanceOfBranch { src, class, target } : "src:R,class:R,target:T" {
        encode => |w| { w.a = src.0; w.b = class.0; w.d = target.0; },
        decode(w) => Ok(Instruction::DynamicInstanceOfBranch { src: Reg(w.a), class: Reg(w.b), target: Target(w.d) }),
    },
    107 => DynamicInstanceOfBranchIf { src, class, target } : "src:R,class:R,target:T" {
        encode => |w| { w.a = src.0; w.b = class.0; w.d = target.0; },
        decode(w) => Ok(Instruction::DynamicInstanceOfBranchIf { src: Reg(w.a), class: Reg(w.b), target: Target(w.d) }),
    },
    108 => InitStaticCall { class, method, binding } : "class:S,method:S,binding:CB" {
        encode => |w| { w.x = encode_class_binding(binding); w.d = class.0; w.e = method.0; },
        decode(w) => Ok(Instruction::InitStaticCall { class: SymbolId(w.d), method: SymbolId(w.e), binding: decode_class_binding(w.x)? }),
    },
    109 => FetchIndex { dst, base, index, path, mode } : "dst:R,base:R,index:R,path:P,mode:FM" {
        encode => |w| { w.x = encode_fetch_mode(mode); w.a = dst.0; w.b = base.0; w.c = index.0; w.d = path.0; },
        decode(w) => Ok(Instruction::FetchIndex { dst: Reg(w.a), base: Reg(w.b), index: Reg(w.c), path: PathId(w.d), mode: decode_fetch_mode(w.x)? }),
    },
    110 => ThisOrNull { dst } : "dst:R" {
        encode => |w| { w.a = dst.0; },
        decode(w) => Ok(Instruction::ThisOrNull { dst: Reg(w.a) }),
    },
    111 => Capture { src, kind } : "src:R,kind:CK" {
        encode => |w| { w.x = encode_capture_kind(kind); w.a = src.0; },
        decode(w) => Ok(Instruction::Capture { src: Reg(w.a), kind: decode_capture_kind(w.x)? }),
    },
    112 => MakeClosure { dst, function, binding } : "dst:R,function:F,binding:ClB" {
        encode => |w| { w.x = encode_closure_binding(binding); w.a = dst.0; w.d = function.0; },
        decode(w) => Ok(Instruction::MakeClosure { dst: Reg(w.a), function: FunctionId(w.d), binding: decode_closure_binding(w.x)? }),
    },
    113 => FetchDynamicClassConstant { dst, class, name } : "dst:R,class:R,name:R" {
        encode => |w| { w.a = dst.0; w.b = class.0; w.c = name.0; },
        decode(w) => Ok(Instruction::FetchDynamicClassConstant { dst: Reg(w.a), class: Reg(w.b), name: Reg(w.c) }),
    },
    114 => Eval { dst, source } : "dst:R,source:R" {
        encode => |w| { w.a = dst.0; w.b = source.0; },
        decode(w) => Ok(Instruction::Eval { dst: Reg(w.a), source: Reg(w.b) }),
    },
    115 => BuiltinUnary { op, dst, src } : "op:BU,dst:R,src:R" {
        encode => |w| {
            let (tag, mask) = encode_builtin_unary(op);
            w.x = tag;
            w.a = dst.0;
            w.b = src.0;
            w.c = mask;
        },
        decode(w) => Ok(Instruction::BuiltinUnary {
            op: decode_builtin_unary(w.x, w.c)?,
            dst: Reg(w.a),
            src: Reg(w.b),
        }),
    },
    116 => NamespacedBuiltin { op, dst, src } : "op:BU,dst:R,src:R" {
        encode => |w| {
            let (tag, mask) = encode_builtin_unary(op);
            w.x = tag;
            w.a = dst.0;
            w.b = src.0;
            w.c = mask;
        },
        decode(w) => Ok(Instruction::NamespacedBuiltin {
            op: decode_builtin_unary(w.x, w.c)?,
            dst: Reg(w.a),
            src: Reg(w.b),
        }),
    },
    117 => GuardBuiltin { function, call } : "function:S,call:T" {
        encode => |w| { w.d = function.0; w.e = call.0; },
        decode(w) => Ok(Instruction::GuardBuiltin { function: SymbolId(w.d), call: Target(w.e) }),
    },
    118 => CallNativeDirect { function, args, count, dst, result } : "function:S,args:R,count:u16,dst:R,result:CR" {
        encode => |w| { w.x = encode_call_result(result); w.a = args.0; w.b = count; w.c = dst.0; w.d = function.0; },
        decode(w) => Ok(Instruction::CallNativeDirect { function: SymbolId(w.d), args: Reg(w.a), count: w.b, dst: Reg(w.c), result: decode_call_result(w.x)? }),
    },
    119 => BuiltinFrame { op, dst } : "op:BF,dst:R" {
        encode => |w| {
            let (tag, extra) = encode_builtin_frame(op);
            w.x = tag;
            w.a = dst.0;
            w.d = extra;
        },
        decode(w) => Ok(Instruction::BuiltinFrame {
            op: decode_builtin_frame(w.x, w.d)?,
            dst: Reg(w.a),
        }),
    },
    120 => ArrayKeyExists { dst, key, array } : "dst:R,key:R,array:R" {
        encode => |w| { w.a = dst.0; w.b = key.0; w.c = array.0; },
        decode(w) => Ok(Instruction::ArrayKeyExists { dst: Reg(w.a), key: Reg(w.b), array: Reg(w.c) }),
    },
    121 => InLiteralArray { dst, needle, haystack, strict } : "dst:R,needle:R,haystack:K,strict:B" {
        encode => |w| { w.x = u8::from(strict); w.a = dst.0; w.b = needle.0; w.d = haystack.0; },
        decode(w) => Ok(Instruction::InLiteralArray { dst: Reg(w.a), needle: Reg(w.b), haystack: ConstId(w.d), strict: decode_bool(w.x, "InLiteralArray::strict")? }),
    },
    122 => InitUserCallback { callback, arguments } : "callback:R,arguments:CA" {
        encode => |w| { w.x = encode_callback_args(arguments); w.a = callback.0; },
        decode(w) => Ok(Instruction::InitUserCallback { callback: Reg(w.a), arguments: decode_callback_args(w.x)? }),
    },
    123 => SendUserCallback { src, arguments } : "src:R,arguments:CA" {
        encode => |w| { w.x = encode_callback_args(arguments); w.a = src.0; },
        decode(w) => Ok(Instruction::SendUserCallback { src: Reg(w.a), arguments: decode_callback_args(w.x)? }),
    },
}

fn ensure_zero_rest(
    w: WireOp,
    x: u8,
    a: u16,
    b: u16,
    c: u16,
    d: u32,
    e: u32,
) -> Result<(), ImageError> {
    if (w.x, w.a, w.b, w.c, w.d, w.e) != (x, a, b, c, d, e) {
        return Err(ImageError::Corrupt(format!(
            "non-zero reserved operand bytes in opcode {}",
            w.op
        )));
    }
    Ok(())
}

fn decode_bool(raw: u8, field: &str) -> Result<bool, ImageError> {
    match raw {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ImageError::Corrupt(format!(
            "invalid boolean value {raw} for {field}"
        ))),
    }
}

fn encode_binary_op(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Modulo => 4,
        BinaryOp::Power => 5,
        BinaryOp::Concat => 6,
        BinaryOp::BitwiseAnd => 7,
        BinaryOp::BitwiseOr => 8,
        BinaryOp::BitwiseXor => 9,
        BinaryOp::ShiftLeft => 10,
        BinaryOp::ShiftRight => 11,
        BinaryOp::LogicalXor => 12,
        BinaryOp::Equal => 13,
        BinaryOp::NotEqual => 14,
        BinaryOp::Identical => 15,
        BinaryOp::NotIdentical => 16,
        BinaryOp::Less => 17,
        BinaryOp::LessEqual => 18,
        BinaryOp::Greater => 19,
        BinaryOp::GreaterEqual => 20,
        BinaryOp::Spaceship => 21,
        BinaryOp::Coalesce => 22,
    }
}

fn decode_binary_op(raw: u8) -> Result<BinaryOp, ImageError> {
    match raw {
        0 => Ok(BinaryOp::Add),
        1 => Ok(BinaryOp::Subtract),
        2 => Ok(BinaryOp::Multiply),
        3 => Ok(BinaryOp::Divide),
        4 => Ok(BinaryOp::Modulo),
        5 => Ok(BinaryOp::Power),
        6 => Ok(BinaryOp::Concat),
        7 => Ok(BinaryOp::BitwiseAnd),
        8 => Ok(BinaryOp::BitwiseOr),
        9 => Ok(BinaryOp::BitwiseXor),
        10 => Ok(BinaryOp::ShiftLeft),
        11 => Ok(BinaryOp::ShiftRight),
        12 => Ok(BinaryOp::LogicalXor),
        13 => Ok(BinaryOp::Equal),
        14 => Ok(BinaryOp::NotEqual),
        15 => Ok(BinaryOp::Identical),
        16 => Ok(BinaryOp::NotIdentical),
        17 => Ok(BinaryOp::Less),
        18 => Ok(BinaryOp::LessEqual),
        19 => Ok(BinaryOp::Greater),
        20 => Ok(BinaryOp::GreaterEqual),
        21 => Ok(BinaryOp::Spaceship),
        22 => Ok(BinaryOp::Coalesce),
        _ => Err(ImageError::Corrupt(format!("invalid BinaryOp {raw}"))),
    }
}

fn encode_unary_op(op: UnaryOp) -> u8 {
    match op {
        UnaryOp::Plus => 0,
        UnaryOp::Negate => 1,
        UnaryOp::Not => 2,
        UnaryOp::BitwiseNot => 3,
    }
}

fn decode_unary_op(raw: u8) -> Result<UnaryOp, ImageError> {
    match raw {
        0 => Ok(UnaryOp::Plus),
        1 => Ok(UnaryOp::Negate),
        2 => Ok(UnaryOp::Not),
        3 => Ok(UnaryOp::BitwiseNot),
        _ => Err(ImageError::Corrupt(format!("invalid UnaryOp {raw}"))),
    }
}

fn encode_cast_kind(kind: CastKind) -> u8 {
    match kind {
        CastKind::Bool => 0,
        CastKind::Int => 1,
        CastKind::Float => 2,
        CastKind::String => 3,
        CastKind::Array => 4,
        CastKind::Object => 5,
        CastKind::Void => 6,
    }
}

fn decode_cast_kind(raw: u8) -> Result<CastKind, ImageError> {
    match raw {
        0 => Ok(CastKind::Bool),
        1 => Ok(CastKind::Int),
        2 => Ok(CastKind::Float),
        3 => Ok(CastKind::String),
        4 => Ok(CastKind::Array),
        5 => Ok(CastKind::Object),
        6 => Ok(CastKind::Void),
        _ => Err(ImageError::Corrupt(format!("invalid CastKind {raw}"))),
    }
}

fn encode_inc_dec(mode: IncDec) -> u8 {
    match mode {
        IncDec::PreIncrement => 0,
        IncDec::PreDecrement => 1,
        IncDec::PostIncrement => 2,
        IncDec::PostDecrement => 3,
    }
}

fn decode_inc_dec(raw: u8) -> Result<IncDec, ImageError> {
    match raw {
        0 => Ok(IncDec::PreIncrement),
        1 => Ok(IncDec::PreDecrement),
        2 => Ok(IncDec::PostIncrement),
        3 => Ok(IncDec::PostDecrement),
        _ => Err(ImageError::Corrupt(format!("invalid IncDec {raw}"))),
    }
}

fn encode_fetch_mode(mode: FetchMode) -> u8 {
    match mode {
        FetchMode::Read => 0,
        FetchMode::Quiet => 1,
        FetchMode::Isset => 2,
        FetchMode::Empty => 3,
        FetchMode::Destructure => 4,
    }
}

fn decode_fetch_mode(raw: u8) -> Result<FetchMode, ImageError> {
    match raw {
        0 => Ok(FetchMode::Read),
        1 => Ok(FetchMode::Quiet),
        2 => Ok(FetchMode::Isset),
        3 => Ok(FetchMode::Empty),
        4 => Ok(FetchMode::Destructure),
        _ => Err(ImageError::Corrupt(format!("invalid FetchMode {raw}"))),
    }
}

fn encode_include_kind(kind: IncludeKind) -> u8 {
    match kind {
        IncludeKind::Include => 0,
        IncludeKind::IncludeOnce => 1,
        IncludeKind::Require => 2,
        IncludeKind::RequireOnce => 3,
    }
}

fn decode_include_kind(raw: u8) -> Result<IncludeKind, ImageError> {
    match raw {
        0 => Ok(IncludeKind::Include),
        1 => Ok(IncludeKind::IncludeOnce),
        2 => Ok(IncludeKind::Require),
        3 => Ok(IncludeKind::RequireOnce),
        _ => Err(ImageError::Corrupt(format!("invalid IncludeKind {raw}"))),
    }
}

fn encode_runtime_error(error: RuntimeError) -> u8 {
    match error {
        RuntimeError::UnhandledMatch => 0,
    }
}

fn decode_runtime_error(raw: u8) -> Result<RuntimeError, ImageError> {
    match raw {
        0 => Ok(RuntimeError::UnhandledMatch),
        _ => Err(ImageError::Corrupt(format!("invalid RuntimeError {raw}"))),
    }
}

fn encode_class_binding(binding: ClassBinding) -> u8 {
    match binding {
        ClassBinding::Forward => 0,
        ClassBinding::Explicit => 1,
    }
}

fn decode_class_binding(raw: u8) -> Result<ClassBinding, ImageError> {
    match raw {
        0 => Ok(ClassBinding::Forward),
        1 => Ok(ClassBinding::Explicit),
        _ => Err(ImageError::Corrupt(format!("invalid ClassBinding {raw}"))),
    }
}

fn encode_finally_entry(reason: FinallyEntry) -> u8 {
    match reason {
        FinallyEntry::Fallthrough => 0,
        FinallyEntry::Return => 1,
        FinallyEntry::Break => 2,
        FinallyEntry::Continue => 3,
        FinallyEntry::Goto => 4,
    }
}

fn decode_finally_entry(raw: u8) -> Result<FinallyEntry, ImageError> {
    match raw {
        0 => Ok(FinallyEntry::Fallthrough),
        1 => Ok(FinallyEntry::Return),
        2 => Ok(FinallyEntry::Break),
        3 => Ok(FinallyEntry::Continue),
        4 => Ok(FinallyEntry::Goto),
        _ => Err(ImageError::Corrupt(format!("invalid FinallyEntry {raw}"))),
    }
}

fn encode_capture_kind(kind: CaptureKind) -> u8 {
    match kind {
        CaptureKind::Value => 0,
        CaptureKind::Reference => 1,
        CaptureKind::Quiet => 2,
    }
}

fn decode_capture_kind(raw: u8) -> Result<CaptureKind, ImageError> {
    match raw {
        0 => Ok(CaptureKind::Value),
        1 => Ok(CaptureKind::Reference),
        2 => Ok(CaptureKind::Quiet),
        _ => Err(ImageError::Corrupt(format!("invalid CaptureKind {raw}"))),
    }
}

fn encode_closure_binding(binding: ClosureBinding) -> u8 {
    match binding {
        ClosureBinding::Bound => 0,
        ClosureBinding::Static => 1,
    }
}

fn decode_closure_binding(raw: u8) -> Result<ClosureBinding, ImageError> {
    match raw {
        0 => Ok(ClosureBinding::Bound),
        1 => Ok(ClosureBinding::Static),
        _ => Err(ImageError::Corrupt(format!("invalid ClosureBinding {raw}"))),
    }
}

fn encode_call_result(result: CallResult) -> u8 {
    match result {
        CallResult::Used => 0,
        CallResult::Discarded => 1,
    }
}

fn decode_call_result(raw: u8) -> Result<CallResult, ImageError> {
    match raw {
        0 => Ok(CallResult::Used),
        1 => Ok(CallResult::Discarded),
        _ => Err(ImageError::Corrupt(format!("invalid CallResult {raw}"))),
    }
}

fn encode_builtin_unary(op: BuiltinUnary) -> (u8, u16) {
    match op {
        BuiltinUnary::Strlen => (0, 0),
        BuiltinUnary::Count => (1, 0),
        BuiltinUnary::Sizeof => (2, 0),
        BuiltinUnary::TypeCheck(mask) => (3, mask.0),
        BuiltinUnary::IsResource => (4, 0),
        BuiltinUnary::GetType => (5, 0),
        BuiltinUnary::GetClass => (6, 0),
        BuiltinUnary::Defined => (7, 0),
        BuiltinUnary::ObjectId => (8, 0),
        BuiltinUnary::StrToLower => (9, 0),
    }
}

fn decode_builtin_unary(tag: u8, mask: u16) -> Result<BuiltinUnary, ImageError> {
    match tag {
        0 => Ok(BuiltinUnary::Strlen),
        1 => Ok(BuiltinUnary::Count),
        2 => Ok(BuiltinUnary::Sizeof),
        3 => Ok(BuiltinUnary::TypeCheck(TypeMask(mask))),
        4 => Ok(BuiltinUnary::IsResource),
        5 => Ok(BuiltinUnary::GetType),
        6 => Ok(BuiltinUnary::GetClass),
        7 => Ok(BuiltinUnary::Defined),
        8 => Ok(BuiltinUnary::ObjectId),
        9 => Ok(BuiltinUnary::StrToLower),
        _ => Err(ImageError::Corrupt(format!("invalid BuiltinUnary {tag}"))),
    }
}

fn encode_builtin_frame(op: BuiltinFrame) -> (u8, u32) {
    match op {
        BuiltinFrame::ArgumentCount => (0, 0),
        BuiltinFrame::ArgumentsFrom(from) => (1, from),
        BuiltinFrame::CalledClass => (2, 0),
        BuiltinFrame::AssertionsDisabled => (3, 0),
    }
}

fn decode_builtin_frame(tag: u8, extra: u32) -> Result<BuiltinFrame, ImageError> {
    match tag {
        0 => Ok(BuiltinFrame::ArgumentCount),
        1 => Ok(BuiltinFrame::ArgumentsFrom(extra)),
        2 => Ok(BuiltinFrame::CalledClass),
        3 => Ok(BuiltinFrame::AssertionsDisabled),
        _ => Err(ImageError::Corrupt(format!("invalid BuiltinFrame {tag}"))),
    }
}

fn encode_callback_args(args: CallbackArguments) -> u8 {
    match args {
        CallbackArguments::Positional => 0,
        CallbackArguments::Array => 1,
    }
}

fn decode_callback_args(raw: u8) -> Result<CallbackArguments, ImageError> {
    match raw {
        0 => Ok(CallbackArguments::Positional),
        1 => Ok(CallbackArguments::Array),
        _ => Err(ImageError::Corrupt(format!(
            "invalid CallbackArguments {raw}"
        ))),
    }
}
