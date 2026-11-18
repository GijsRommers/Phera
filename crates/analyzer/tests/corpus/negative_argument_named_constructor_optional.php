<?php
// Rule: argument.unknown; deliberate single error: argument_named_constructor_optional.
class C { function __construct($first = 0) {} } new C(unavailable: 1);
