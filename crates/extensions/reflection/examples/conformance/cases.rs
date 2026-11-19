//! Cross representative declarations with each public Reflection method and multiple argument shapes.

pub(crate) struct Surface {
    pub reflector: &'static str,
    pub receivers: Vec<String>,
    pub setup: Option<&'static str>,
}

impl Surface {
    pub fn arguments(&self, method: &str) -> &'static str {
        match method {
            "getAttributes" => {
                "[[], ['AuditMarker'], ['AuditMarker', 2], [null, -1], ['Missing', 2]]"
            }
            "getMethod" | "hasMethod" => "[[], ['method'], ['REQUIRED'], ['doesNotExist'], [null]]",
            "getProperty"
            | "hasProperty"
            | "getStaticPropertyValue"
            | "getReflectionConstant"
            | "hasConstant"
            | "getConstant" => {
                "[[], ['value'], ['id'], ['CODE'], ['hidden'], [''], [\"x\\0y\"], [[]]]"
            }
            "getMethods" | "getProperties" | "getConstants" | "getReflectionConstants" => {
                "[[], [null], [0], [1], [4], [16], [17], [-1], [[]]]"
            }
            "isSubclassOf" | "implementsInterface" => {
                "[[], ['BaseClass'], ['InterfaceA'], ['Subject'], ['missing'], [new ReflectionClass('Subject')], [new stdClass()]]"
            }
            "isInstance"
            | "isLazy"
            | "isUninitializedLazyObject"
            | "getRawValue"
            | "skipLazyInitialization"
            | "getLazyInitializer"
            | "initializeLazyObject"
            | "markLazyObjectAsInitialized" => "[[], [null], [new Subject()], [new stdClass()]]",
            "newInstance" => "[[], [3], ['bad']]",
            "newInstanceArgs" => {
                "[[], [[]], [['id' => 3]], [[3]], [['nonexistent' => 4]], ['bad']]"
            }
            "setStaticPropertyValue" => "[[], ['value', 9], ['none', 1], ['value', []]]",
            "createFromMethodName" => {
                "[[], ['Subject::method'], ['noClass::method'], ['noSeparator']]"
            }
            "invoke" | "getClosure" if self.reflector == "ReflectionMethod" => {
                "[[], [null], [new Subject()], [new Subject(), 3], [new stdClass()]]"
            }
            "invoke" => "[[], [3], ['bad'], ['value' => 5]]",
            "invokeArgs" if self.reflector == "ReflectionMethod" => {
                "[[], [null, []], [new Subject(), []], [new Subject(), ['value' => 3]], [new stdClass(), []]]"
            }
            "invokeArgs" => "[[], [[]], [[3]], [['value' => 5]], [['bogus' => 3]], [null]]",
            "getValue" | "isInitialized" if self.reflector == "ReflectionProperty" => {
                "[[], [null], [new Subject()], [new stdClass()]]"
            }
            "setValue" | "setRawValue" | "setRawValueWithoutLazyInitialization" => {
                "[[], [3], [null, '3'], [new Subject(), '3'], [new stdClass(), 3]]"
            }
            "setAccessible" => "[[], [true], [false], [[]]]",
            "fromArrayElement" => "[[], [[1], 0], [[1], '0'], [[], -1]]",
            "getModifierNames" => "[[], [0], [1], [3], [65536], [-1], [PHP_INT_MAX], [[]]]",
            "getCase" | "hasCase" => "[[], ['First'], ['Missing'], [[]]]",
            "getTrace" => "[[], [0], [-1], [[]]]",
            "__construct" if self.reflector == "ReflectionClass" => {
                "[[], ['Subject'], ['missing'], [new Subject()], [null], [true], [[]], [\"x\\0y\"]]"
            }
            "__construct" if self.reflector == "ReflectionMethod" => {
                "[[], ['Subject::method'], ['Subject', 'method'], ['Subject', 'missing'], [new Subject(), 'method'], [[]]]"
            }
            "__construct" if self.reflector == "ReflectionParameter" => {
                "[[], ['testFunction', 0], ['testFunction', 'more'], ['testFunction', PHP_INT_MAX], ['testFunction', 'missing'], [[], 1]]"
            }
            "__construct" => "[[], [null], ['nonexistent'], [[]]]",
            _ => "[[]]",
        }
    }
}

pub(crate) fn surfaces() -> Vec<Surface> {
    let classes = [
        "BaseClass",
        "MiddleClass",
        "Subject",
        "InterfaceA",
        "InterfaceB",
        "Inaccessible",
        "Immutable",
        "FinalClass",
        "Typed",
        "Asymmetric",
        "Untyped",
        "PrivateProperty",
        "ProtectedProperty",
        "Uninitialized",
        "PublicStatic",
        "ProtectedStatic",
        "PrivateStatic",
        "InheritedPrivate",
        "InheritedProtected",
        "InheritedPublic",
        "FinalProperty",
        "ReadOnlyProperty",
        "ProtectedSetProperty",
        "PrivateSetProperty",
        "OptionalObject",
        "EmptyContract",
        "CountableObject",
        "UnionProperty",
        "MixedProperty",
        "NullableProperty",
        "EmptyClass",
        "PrivateMethod",
        "stdClass",
        "Exception",
        "DateTimeImmutable",
        "ArrayAccess",
        "Countable",
        "PDO",
        "Closure",
        "ReflectionClass",
    ];
    let class_targets: Vec<_> = classes
        .iter()
        .map(|name| format!("new ReflectionClass('{name}')"))
        .collect();
    let mut result = vec![Surface {
        reflector: "ReflectionClass",
        receivers: class_targets,
        setup: None,
    }];
    for (reflector, receivers) in [
        (
            "ReflectionObject",
            &[
                "new ReflectionObject(new Subject())",
                "new ReflectionObject(new stdClass())",
            ][..],
        ),
        (
            "ReflectionFunctionAbstract",
            &[
                "new ReflectionFunction('testFunction')",
                "new ReflectionMethod('Subject', 'method')",
            ],
        ),
        (
            "ReflectionFunction",
            &[
                "new ReflectionFunction('testFunction')",
                "new ReflectionFunction('testStatic')",
                "new ReflectionFunction(testClosure())",
                "new ReflectionFunction('strlen')",
            ],
        ),
        (
            "ReflectionMethod",
            &[
                "new ReflectionMethod('Subject', 'method')",
                "new ReflectionMethod('Subject', 'required')",
                "new ReflectionMethod('Subject', 'rest')",
                "new ReflectionMethod('Subject', 'reference')",
                "new ReflectionMethod('ArrayAccess', 'offsetGet')",
                "new ReflectionMethod('PDO', 'query')",
            ],
        ),
        (
            "ReflectionProperty",
            &[
                "new ReflectionProperty('Subject', 'id')",
                "new ReflectionProperty('Subject', 'value')",
                "new ReflectionProperty('Typed', 'integer')",
                "new ReflectionProperty('Asymmetric', 'id')",
                "new ReflectionProperty('ProtectedProperty', 'value')",
            ],
        ),
        (
            "ReflectionParameter",
            &[
                "new ReflectionParameter('testFunction', 0)",
                "new ReflectionParameter('testFunction', 'more')",
                "new ReflectionParameter(['Subject', 'method'], 'value')",
                "new ReflectionParameter(['Subject', 'reference'], 0)",
                "new ReflectionParameter('strlen', 0)",
            ],
        ),
        (
            "ReflectionType",
            &[
                "(new ReflectionFunction('testFunction'))->getReturnType()",
                "(new ReflectionFunction('testIntersection'))->getReturnType()",
            ],
        ),
        (
            "ReflectionNamedType",
            &[
                "(new ReflectionFunction('testFunction'))->getReturnType()",
                "(new ReflectionProperty('NullableProperty', 'value'))->getType()",
                "(new ReflectionParameter('strlen', 0))->getType()",
            ],
        ),
        (
            "ReflectionUnionType",
            &[
                "(new ReflectionMethod('Subject', 'method'))->getReturnType()",
                "(new ReflectionFunction('testIntersection'))->getReturnType()",
            ],
        ),
        (
            "ReflectionIntersectionType",
            &["(new ReflectionParameter('testIntersection', 0))->getType()"],
        ),
        (
            "ReflectionAttribute",
            &[
                "(new ReflectionClass('Subject'))->getAttributes()[0]",
                "(new ReflectionClass('Attribute'))->getAttributes()[0]",
            ],
        ),
        (
            "ReflectionClassConstant",
            &[
                "new ReflectionClassConstant('Subject', 'CODE')",
                "new ReflectionClassConstant('BaseClass', 'TEXT')",
                "new ReflectionClassConstant('Attribute', 'TARGET_ALL')",
            ],
        ),
        (
            "ReflectionExtension",
            &[
                "new ReflectionExtension('date')",
                "new ReflectionExtension('Reflection')",
                "new ReflectionExtension('PDO')",
            ],
        ),
        (
            "ReflectionException",
            &["new ReflectionException('missing')"],
        ),
        (
            "ReflectionConstant",
            &[
                "new ReflectionConstant('AUDIT_CONSTANT')",
                "new ReflectionConstant('PHP_VERSION')",
            ],
        ),
        (
            "ReflectionReference",
            &["ReflectionReference::fromArrayElement($GLOBALS['refs'], 0)"],
        ),
        ("Reflection", &["new Reflection()"]),
        ("Reflector", &["new ReflectionClass('Subject')"]),
        (
            "ReflectionEnum",
            &[
                "new ReflectionEnum('UnitSample')",
                "new ReflectionEnum('BackedSample')",
            ],
        ),
        (
            "ReflectionEnumUnitCase",
            &["new ReflectionEnumUnitCase('UnitSample', 'First')"],
        ),
        (
            "ReflectionEnumBackedCase",
            &["new ReflectionEnumBackedCase('BackedSample', 'First')"],
        ),
        (
            "ReflectionGenerator",
            &["new ReflectionGenerator(oracleGenerator())"],
        ),
    ] {
        let setup = match reflector {
            "ReflectionEnum" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => Some(
                "\ntry { eval('enum UnitSample { case First; } enum BackedSample: string { case First = \"first\"; }'); } catch (Throwable $e) {}\n",
            ),
            "ReflectionGenerator" => Some(
                "\ntry { eval('function oracleGenerator() { yield 1; }'); } catch (Throwable $e) {}\n",
            ),
            _ => None,
        };
        result.push(Surface {
            reflector,
            receivers: receivers
                .iter()
                .map(|receiver| (*receiver).to_owned())
                .collect(),
            setup,
        });
    }
    result
}
