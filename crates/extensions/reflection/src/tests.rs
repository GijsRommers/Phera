use std::rc::Rc;

use phera_extension_api::reflection::{
    AttributeSite, ClassInfo, ClassSelector, ConstantInfo, ExtensionInfo, FunctionInfo,
    FunctionTarget, GeneratorQuery, Initializer, LazyReflectionRead, MemberInfo, MemberKind,
    Members, ParameterDefault, ParameterInfo, PropertyHandle, PropertyHookKind, PropertyRead,
    PropertyWrite, ReflectedClassKind, ReflectionContext, SourceInfo, lazy,
};
use phera_extension_api::testing::{TestContext, call_method, instantiate};
use phera_extension_api::{
    ArrayKey, ClassEntry, CompileDiagnostic, CompileDiagnosticSite, Context, Diagnostics,
    Extension, LocalFuture, MethodEntry, NativeError, NativeMethod, NativeResult, Object, PhpArray,
    Registry, RequestState, Value,
};

use crate::class::ReflectedClass;
use crate::parameter::ReflectedParameter;
use crate::{Reflection, bind};

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.class(ClassEntry::class("Exception"));
    registry.class(ClassEntry::interface("Stringable"));
    Reflection.register(&mut registry);
    registry
}

fn find_method<'a>(registry: &'a Registry, class: &str, name: &str) -> &'a MethodEntry {
    let mut current = registry.get_class(class);
    while let Some(entry) = current {
        if let Some(method) = entry
            .methods
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(name))
        {
            return method;
        }
        current = entry.parent.and_then(|parent| registry.get_class(parent));
    }
    panic!("method {class}::{name}() not found");
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (a, b) if a.str_ref().is_some() && b.str_ref().is_some() => {
            a.str_ref().map(|s| s.as_bytes()) == b.str_ref().map(|s| s.as_bytes())
        }
        _ => false,
    }
}

fn assert_direct_matches_general(registry: &Registry, object: &Rc<Object>, methods: &[&str]) {
    let mut context = TestContext::default();
    for method in methods {
        let entry = find_method(registry, object.class.name(), method);
        let direct = entry.direct.unwrap_or_else(|| {
            panic!(
                "{}::{method}() should have a direct getter",
                object.class.name()
            )
        });
        let direct_value = direct(object, &[]).unwrap_or_else(|| {
            panic!(
                "{}::{method}() direct getter unexpectedly returned None",
                object.class.name()
            )
        });
        let general_value = call_method(registry, &mut context, object, method, Vec::new())
            .unwrap_or_else(|error| {
                panic!(
                    "{}::{method}() general handler failed: {error:?}",
                    object.class.name()
                )
            });
        assert!(
            values_equal(&direct_value, &general_value),
            "{}::{method}() mismatch: direct={direct_value:?}, general={general_value:?}",
            object.class.name()
        );
        assert!(
            direct(object, &[Some(Value::Int(1))]).is_none(),
            "{}::{method}() direct getter should decline when given arguments",
            object.class.name()
        );
    }
}

fn assert_uninitialized_falls_back(registry: &Registry, class: &str, methods: &[&str]) {
    let object = instantiate(registry, class).expect("instantiate reflection class");
    let mut context = TestContext::default();
    for method in methods {
        let entry = find_method(registry, class, method);
        let direct = entry
            .direct
            .unwrap_or_else(|| panic!("{class}::{method}() should have a direct getter"));
        assert!(
            direct(&object, &[]).is_none(),
            "{class}::{method}() direct getter must return None when uninitialized"
        );
        assert!(
            call_method(registry, &mut context, &object, method, Vec::new()).is_err(),
            "{class}::{method}() general handler must fail when uninitialized"
        );
    }
}

fn sample_class_info(
    id: u32,
    name: &str,
    kind: ReflectedClassKind,
    modifiers: i64,
) -> Rc<ClassInfo> {
    Rc::new(ClassInfo {
        id,
        name: name.to_owned(),
        name_value: Value::from(name),
        kind,
        source: Some(SourceInfo {
            file: "app.php".into(),
            start_line: 10,
            end_line: 30,
            doc_comment: None,
        }),
        extension: None,
        parent: None,
        interfaces: Vec::new(),
        traits: Vec::new(),
        enum_backing: None,
        trait_aliases: Vec::new(),
        modifiers,
        is_abstract: modifiers & 64 != 0,
        instantiable: kind == ReflectedClassKind::Class && modifiers & 64 == 0,
        cloneable: kind == ReflectedClassKind::Class && modifiers & 64 == 0,
        iterable: false,
        anonymous: false,
        constructor: Some(name.to_owned()),
        attributes: Vec::new(),
    })
}

fn sample_function_info(
    name: &str,
    member: Option<MemberInfo>,
    is_static: bool,
    parameters: Vec<ParameterInfo>,
) -> Rc<FunctionInfo> {
    Rc::new(FunctionInfo {
        target: FunctionTarget::Named(name.to_owned()),
        name: name.to_owned(),
        member,
        source: Some(SourceInfo {
            file: "app.php".into(),
            start_line: 1,
            end_line: 5,
            doc_comment: None,
        }),
        extension: None,
        parameters,
        returns_reference: false,
        return_type: None,
        tentative_return_type: None,
        is_closure: false,
        anonymous: false,
        is_static,
        is_generator: false,
        deprecated: false,
        closure_this: None,
        closure_scope: None,
        closure_called: None,
        attributes: Vec::new(),
    })
}

fn sample_parameter(name: &str, position: usize) -> ParameterInfo {
    ParameterInfo {
        name: name.to_owned(),
        position,
        declared_type: Some("int".into()),
        by_reference: false,
        can_pass_by_value: true,
        variadic: false,
        optional: false,
        promoted: false,
        default: None,
        default_constant: None,
        default_display: None,
        internal: false,
        attributes: Vec::new(),
    }
}

#[test]
fn reflection_parameter_direct_getters_match_general_and_edge_cases() {
    let registry = registry();
    let base_methods = [
        "getName",
        "getPosition",
        "hasType",
        "allowsNull",
        "isOptional",
        "isVariadic",
        "isDefaultValueAvailable",
        "isPromoted",
        "isPassedByReference",
        "canBePassedByValue",
    ];
    let all_methods = [
        "getName",
        "getPosition",
        "hasType",
        "allowsNull",
        "isOptional",
        "isVariadic",
        "isDefaultValueAvailable",
        "isPromoted",
        "isPassedByReference",
        "canBePassedByValue",
        "isDefaultValueConstant",
    ];

    let mut required = sample_parameter("id", 0);
    required.declared_type = Some("int".into());

    let mut promoted_const = sample_parameter("label", 1);
    promoted_const.declared_type = Some("?string".into());
    promoted_const.optional = true;
    promoted_const.promoted = true;
    promoted_const.default = Some(ParameterDefault::Literal(Value::from("ok")));
    promoted_const.default_constant = Some("App\\Config::DEFAULT_LABEL".into());

    let mut by_ref_literal = sample_parameter("buffer", 2);
    by_ref_literal.declared_type = Some("mixed".into());
    by_ref_literal.by_reference = true;
    by_ref_literal.can_pass_by_value = false;
    by_ref_literal.optional = true;
    by_ref_literal.default = Some(ParameterDefault::Literal(Value::Int(7)));

    let mut variadic = sample_parameter("rest", 3);
    variadic.declared_type = None;
    variadic.variadic = true;
    variadic.optional = true;

    let function = sample_function_info(
        "build",
        None,
        false,
        vec![required, promoted_const, by_ref_literal, variadic],
    );

    let mut context = TestContext::default();
    for (position, has_default) in [(0, false), (1, true), (2, true), (3, false)] {
        let object = instantiate(&registry, "ReflectionParameter").unwrap();
        bind(
            &object,
            Some(&function.parameters[position].name),
            ReflectedParameter {
                function: Rc::clone(&function),
                position,
            },
        )
        .unwrap();

        if has_default {
            assert_direct_matches_general(&registry, &object, &all_methods);
        } else {
            assert_direct_matches_general(&registry, &object, &base_methods);
            let entry = find_method(&registry, "ReflectionParameter", "isDefaultValueConstant");
            let direct = entry.direct.expect("direct isDefaultValueConstant");
            assert!(
                direct(&object, &[]).is_none(),
                "isDefaultValueConstant must decline when parameter has no default"
            );
            let error = call_method(
                &registry,
                &mut context,
                &object,
                "isDefaultValueConstant",
                Vec::new(),
            )
            .expect_err("general handler must throw ReflectionException");
            assert!(matches!(
                error,
                NativeError::Throw {
                    class: "ReflectionException",
                    ..
                }
            ));
        }
    }

    // Out-of-bounds parameter offset falls back to the general handler.
    let invalid = instantiate(&registry, "ReflectionParameter").unwrap();
    bind(
        &invalid,
        Some("missing"),
        ReflectedParameter {
            function,
            position: 99,
        },
    )
    .unwrap();
    for method in all_methods {
        let entry = find_method(&registry, "ReflectionParameter", method);
        assert!(entry.direct.unwrap()(&invalid, &[]).is_none());
        assert!(call_method(&registry, &mut context, &invalid, method, Vec::new()).is_err());
    }

    assert_uninitialized_falls_back(&registry, "ReflectionParameter", &all_methods);
}

#[test]
fn reflection_type_direct_getters_match_general_and_edge_cases() {
    let registry = registry();
    let named_methods = ["allowsNull", "__toString", "getName", "isBuiltin"];
    let type_methods = ["allowsNull", "__toString"];
    let mut context = TestContext::default();

    for spec in [
        "int",
        "?int",
        "App\\Service",
        "?App\\Service",
        "mixed",
        "static",
    ] {
        let Value::Object(object) = crate::types::make(&mut context, Some(spec)).unwrap() else {
            panic!("expected ReflectionNamedType object for {spec}");
        };
        assert_eq!(object.class.name(), "ReflectionNamedType");
        assert_direct_matches_general(&registry, &object, &named_methods);
    }

    for spec in ["int|string|null", "int|string", "Countable&Iterator"] {
        let Value::Object(object) = crate::types::make(&mut context, Some(spec)).unwrap() else {
            panic!("expected ReflectionType object for {spec}");
        };
        assert_direct_matches_general(&registry, &object, &type_methods);
    }

    assert_uninitialized_falls_back(&registry, "ReflectionNamedType", &named_methods);
    assert_uninitialized_falls_back(&registry, "ReflectionUnionType", &type_methods);
    assert_uninitialized_falls_back(&registry, "ReflectionIntersectionType", &type_methods);
}

#[test]
fn reflection_class_direct_getters_match_general_and_edge_cases() {
    let registry = registry();
    let methods = [
        "getName",
        "getShortName",
        "getNamespaceName",
        "inNamespace",
        "isInternal",
        "isUserDefined",
        "isAnonymous",
        "isInstantiable",
        "isCloneable",
        "isIterable",
        "isIterateable",
        "isAbstract",
        "isFinal",
        "isReadOnly",
        "isInterface",
        "isTrait",
        "isEnum",
        "getModifiers",
    ];

    let mut concrete = (*sample_class_info(
        10,
        "App\\Services\\OrderService",
        ReflectedClassKind::Class,
        32 | 65536,
    ))
    .clone();
    concrete.iterable = true;

    let mut internal_abstract =
        (*sample_class_info(11, "Iterator", ReflectedClassKind::Interface, 0)).clone();
    internal_abstract.source = None;
    internal_abstract.is_abstract = true;

    let mut anonymous = (*sample_class_info(
        12,
        "class@anonymous\0/app/index.php:10$0",
        ReflectedClassKind::Class,
        0,
    ))
    .clone();
    anonymous.anonymous = true;

    let trait_info =
        (*sample_class_info(13, "App\\Concerns\\Logs", ReflectedClassKind::Trait, 0)).clone();
    let enum_info =
        (*sample_class_info(14, "App\\Enums\\Status", ReflectedClassKind::Enum, 32)).clone();

    for info in [
        concrete,
        internal_abstract,
        anonymous,
        trait_info,
        enum_info,
    ] {
        let info = Rc::new(info);
        let object = instantiate(&registry, "ReflectionClass").unwrap();
        bind(
            &object,
            Some(&info.name),
            ReflectedClass {
                info: Rc::clone(&info),
                object: None,
            },
        )
        .unwrap();
        assert_direct_matches_general(&registry, &object, &methods);
    }

    assert_uninitialized_falls_back(&registry, "ReflectionClass", &methods);
    assert_uninitialized_falls_back(&registry, "ReflectionObject", &methods);
    assert_uninitialized_falls_back(&registry, "ReflectionEnum", &methods);
}

#[test]
fn reflection_method_direct_getters_match_general_and_edge_cases() {
    let registry = registry();
    let all_methods = [
        "isPublic",
        "isProtected",
        "isPrivate",
        "isAbstract",
        "isFinal",
        "isStatic",
        "isConstructor",
        "isDestructor",
        "getModifiers",
    ];
    let member_only = [
        "isPublic",
        "isProtected",
        "isPrivate",
        "isAbstract",
        "isFinal",
        "getModifiers",
    ];

    let fixtures = [
        ("__construct", 1, false),
        ("__destruct", 2 | 32, false),
        ("boot", 4 | 16, true),
        ("handle", 1 | 64, false),
    ];

    for (name, modifiers, is_static) in fixtures {
        let member = MemberInfo {
            name: name.to_owned(),
            class: "App\\Kernel".into(),
            lookup_class: "App\\Kernel".into(),
            modifiers,
            ..MemberInfo::default()
        };
        let function = sample_function_info(name, Some(member), is_static, Vec::new());
        let object = instantiate(&registry, "ReflectionMethod").unwrap();
        bind(&object, Some(name), function).unwrap();
        assert_direct_matches_general(&registry, &object, &all_methods);
    }

    // A ReflectionMethod whose FunctionInfo has member: None must decline on member-dependent getters.
    let non_member = sample_function_info("plain", None, true, Vec::new());
    let object = instantiate(&registry, "ReflectionMethod").unwrap();
    bind(&object, Some("plain"), non_member).unwrap();
    assert_direct_matches_general(
        &registry,
        &object,
        &["isStatic", "isConstructor", "isDestructor"],
    );

    let mut context = TestContext::default();
    for method in member_only {
        let entry = find_method(&registry, "ReflectionMethod", method);
        assert!(
            entry.direct.unwrap()(&object, &[]).is_none(),
            "ReflectionMethod::{method}() must decline when FunctionInfo.member is None"
        );
        let error = call_method(&registry, &mut context, &object, method, Vec::new())
            .expect_err("general handler must throw ReflectionException");
        assert!(matches!(
            error,
            NativeError::Throw {
                class: "ReflectionException",
                ..
            }
        ));
    }

    assert_uninitialized_falls_back(&registry, "ReflectionMethod", &all_methods);
}

#[test]
fn keep_closure_skips_fresh_objects_and_clears_on_rebind() {
    let registry = registry();
    let retained_key = ArrayKey::str(crate::RETAINED);

    // Freshly allocated parameter for a non-closure function never creates RETAINED.
    let mut context = TestContext::default();
    let function = sample_function_info("resolve", None, false, vec![sample_parameter("id", 0)]);
    let Value::Object(param) = crate::parameter::make(&mut context, function, 0).unwrap() else {
        panic!("expected ReflectionParameter");
    };
    assert!(!param.has(&retained_key));

    // Fresh unbound ReflectionFunction: non-closure keep_closure is a no-op; closure sets RETAINED;
    // after bind, non-closure keep_closure removes RETAINED.
    let rf = instantiate(&registry, "ReflectionFunction").unwrap();
    let closure = instantiate(&registry, "ReflectionClass").unwrap();
    crate::function::keep_closure(&rf, &FunctionTarget::Named("strlen".into()));
    assert!(!rf.has(&retained_key));

    crate::function::keep_closure(&rf, &FunctionTarget::Closure(Rc::downgrade(&closure)));
    assert!(rf.has(&retained_key));

    let fn_info = sample_function_info("strlen", None, false, Vec::new());
    bind(&rf, Some("strlen"), fn_info).unwrap();
    crate::function::keep_closure(&rf, &FunctionTarget::Named("strlen".into()));
    assert!(!rf.has(&retained_key));
}

struct RecordingContext {
    inner: TestContext,
    targets: Vec<FunctionTarget>,
    reply: Rc<FunctionInfo>,
}

impl Context for RecordingContext {
    fn write(&mut self, bytes: &[u8]) {
        self.inner.write(bytes);
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self.inner.diagnostics()
    }

    fn compile_diagnostic(
        &mut self,
        diagnostic: &CompileDiagnostic,
        site: CompileDiagnosticSite<'_>,
    ) {
        self.inner.compile_diagnostic(diagnostic, site);
    }

    fn state(&mut self) -> &mut RequestState {
        self.inner.state()
    }

    fn strict_types(&self) -> bool {
        self.inner.strict_types()
    }

    fn create_object(&mut self, class: &str, properties: PhpArray) -> NativeResult {
        self.inner.create_object(class, properties)
    }

    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        self.inner.call(callable, arguments)
    }

    fn reflection(&mut self) -> Option<&mut dyn ReflectionContext> {
        Some(self)
    }
}

impl ReflectionContext for RecordingContext {
    fn reflect_generator_function(&self, _: &Object) -> Result<FunctionTarget, NativeError> {
        unreachable!()
    }

    fn reflect_generator(&mut self, _: &Rc<Object>, _: GeneratorQuery) -> NativeResult {
        unreachable!()
    }

    fn reflect_constant(&self, _: &str) -> Result<ConstantInfo, NativeError> {
        unreachable!()
    }

    fn reflect_constant_value(&self, _: &str) -> NativeResult {
        unreachable!()
    }

    fn reflect_function(
        &mut self,
        target: &FunctionTarget,
    ) -> Result<Rc<FunctionInfo>, NativeError> {
        self.targets.push(target.clone());
        Ok(Rc::clone(&self.reply))
    }

    fn reflect_invoke(
        &mut self,
        _: FunctionTarget,
        _: Option<Rc<Object>>,
        _: PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        unreachable!()
    }

    fn reflect_closure(&mut self, _: FunctionTarget, _: Option<Rc<Object>>) -> NativeResult {
        unreachable!()
    }

    fn reflect_closure_uses(&self, _: &FunctionTarget) -> NativeResult {
        unreachable!()
    }

    fn reflect_statics(&mut self, _: &FunctionTarget) -> NativeResult {
        unreachable!()
    }

    fn reflect_prototype(&self, _: &MemberInfo) -> Option<MemberInfo> {
        unreachable!()
    }

    fn reflect_extension(&self, _: &str) -> Option<ExtensionInfo> {
        unreachable!()
    }

    fn reflect_dynamic_property_write(&mut self, _: Rc<Object>, _: &str, _: Value) -> NativeResult {
        unreachable!()
    }

    fn reflect_loaded_class(&mut self, _: &str) -> Option<Rc<ClassInfo>> {
        unreachable!()
    }

    fn reflect_object_class(&mut self, _: &Object) -> Option<Rc<ClassInfo>> {
        unreachable!()
    }

    fn reflect_class<'a>(
        &'a mut self,
        _: &'a str,
    ) -> LocalFuture<'a, Result<Rc<ClassInfo>, NativeError>> {
        unreachable!()
    }

    fn reflect_members(&mut self, _: ClassSelector<'_>, _: MemberKind) -> Members {
        unreachable!()
    }

    fn reflect_after_initialization(
        &mut self,
        _: Rc<Object>,
        _: LazyReflectionRead,
    ) -> NativeResult {
        unreachable!()
    }

    fn reflect_instance_has_property(&mut self, _: Rc<Object>, _: String) -> NativeResult {
        unreachable!()
    }

    fn reflect_class_constant(&mut self, _: &str, _: &str) -> NativeResult {
        unreachable!()
    }

    fn reflect_construct(
        &mut self,
        _: &ClassInfo,
        _: Option<PhpArray>,
    ) -> LocalFuture<'_, NativeResult> {
        unreachable!()
    }

    fn reflect_new_instance_without_constructor(&mut self, _: &ClassInfo) -> NativeResult {
        unreachable!()
    }

    fn reflect_make_lazy(
        &mut self,
        _: &ClassInfo,
        _: lazy::MakeLazy,
    ) -> LocalFuture<'_, NativeResult> {
        unreachable!()
    }

    fn reflect_lazy_object(&mut self, _: Rc<Object>, _: lazy::LazyClassQuery) -> NativeResult {
        unreachable!()
    }

    fn reflect_lazy_property(
        &mut self,
        _: &MemberInfo,
        _: Rc<Object>,
        _: lazy::LazyPropertyOperation,
    ) -> NativeResult {
        unreachable!()
    }

    fn reflect_write_lazy_property(
        &mut self,
        _: Rc<MemberInfo>,
        _: Rc<Object>,
        _: Value,
    ) -> LocalFuture<'_, NativeResult> {
        unreachable!()
    }

    fn reflect_construct_attribute(
        &mut self,
        _: &ClassInfo,
        _: PhpArray,
        _: Option<AttributeSite>,
    ) -> LocalFuture<'_, NativeResult> {
        unreachable!()
    }

    fn reflect_evaluate(&mut self, _: Initializer) -> LocalFuture<'_, NativeResult> {
        unreachable!()
    }

    fn reflect_property_hook(&self, _: &MemberInfo, _: PropertyHookKind) -> Option<FunctionTarget> {
        unreachable!()
    }

    fn reflect_hook_prototype(
        &self,
        _: PropertyHandle,
        _: PropertyHookKind,
    ) -> Option<FunctionTarget> {
        unreachable!()
    }

    fn reflect_property_settable_type(&self, _: &MemberInfo) -> Option<String> {
        unreachable!()
    }

    fn reflect_property_read(
        &mut self,
        _: &MemberInfo,
        _: Option<Rc<Object>>,
        _: PropertyRead,
    ) -> NativeResult {
        unreachable!()
    }

    fn reflect_property_write(
        &mut self,
        _: &MemberInfo,
        _: Option<Rc<Object>>,
        _: Value,
        _: PropertyWrite,
    ) -> NativeResult {
        unreachable!()
    }
}

#[test]
fn class_members_lookup_uses_linked_method_target() {
    let registry = registry();
    let class_info = sample_class_info(42, "App\\Container", ReflectedClassKind::Class, 0);
    let rc = instantiate(&registry, "ReflectionClass").unwrap();
    bind(
        &rc,
        Some(&class_info.name),
        ReflectedClass {
            info: Rc::clone(&class_info),
            object: None,
        },
    )
    .unwrap();

    let member = MemberInfo {
        name: "__construct".into(),
        class: "App\\Container".into(),
        lookup_class: "App\\Container".into(),
        modifiers: 1,
        ..MemberInfo::default()
    };
    let reply = sample_function_info("__construct", Some(member), false, Vec::new());
    let mut context = RecordingContext {
        inner: TestContext::default(),
        targets: Vec::new(),
        reply,
    };

    for (method_name, args) in [
        ("getConstructor", vec![]),
        ("getMethod", vec![Value::from("make")]),
        ("hasMethod", vec![Value::from("make")]),
    ] {
        let entry = find_method(&registry, "ReflectionClass", method_name);
        let NativeMethod::Sync(handler) = entry.handler else {
            panic!("expected sync handler for {method_name}");
        };
        handler(&mut context, Some(&rc), &args).unwrap();
    }

    assert_eq!(context.targets.len(), 3);
    assert!(matches!(
        &context.targets[0],
        FunctionTarget::LinkedMethod { class: 42, name } if name == "__construct"
    ));
    assert!(matches!(
        &context.targets[1],
        FunctionTarget::LinkedMethod { class: 42, name } if name == "make"
    ));
    assert!(matches!(
        &context.targets[2],
        FunctionTarget::LinkedMethod { class: 42, name } if name == "make"
    ));
}
