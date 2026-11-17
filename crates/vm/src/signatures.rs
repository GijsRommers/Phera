//! Signatures are linked once, so `f(1)` copies their address rather than owners.
//!
//! Parameter names, types and diagnostics stay here for the entire request.

use crate::calls::call_state::ParameterSpec;

/// A stable address for immutable argument-routing metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SignatureIndex(pub(crate) u32);

/// Native handlers also live behind numeric addresses, keeping prepared calls small.
#[derive(Clone, Copy)]
pub(crate) struct NativeIndex(pub(crate) u32);

/// The argument declaration shared by every invocation of a callable.
pub(crate) struct Signature {
    pub(crate) name: String,
    pub(crate) closure_invoke: SignatureIndex,
    pub(crate) parameters: Vec<ParameterSpec>,
}

/// A flat request-owned table; linking inserts and execution only borrows.
#[derive(Default)]
pub(crate) struct Signatures {
    entries: Vec<Signature>,
    native: Vec<phera_extension_api::FunctionEntry>,
}

impl Signatures {
    /// Declaration and native registration each create their routing data once.
    pub(crate) fn insert(
        &mut self,
        name: String,
        parameters: Vec<ParameterSpec>,
    ) -> SignatureIndex {
        let index = SignatureIndex(self.entries.len() as u32);
        let closure_invoke = SignatureIndex(index.0 + 1);
        self.entries.push(Signature {
            name,
            parameters: parameters.clone(),
            closure_invoke,
        });
        self.entries.push(Signature {
            name: "Closure::__invoke".into(),
            parameters,
            closure_invoke,
        });
        index
    }

    /// Numeric signature addresses have no guard or ownership operation.
    pub(crate) fn get(&self, index: SignatureIndex) -> &Signature {
        &self.entries[index.0 as usize]
    }
}

impl Signatures {
    /// Native registration publishes its handler once for the request.
    pub(crate) fn insert_native(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
    ) -> NativeIndex {
        let index = NativeIndex(self.native.len() as u32);
        self.native.push(entry);
        index
    }

    /// The extension boundary copies its API entry only when invoking native code.
    pub(crate) fn native(&self, index: NativeIndex) -> phera_extension_api::FunctionEntry {
        self.native[index.0 as usize]
    }
}
