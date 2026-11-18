<?php
// Rule: argument.unknown; deliberate single error: argument_named_parent_optional.
class A { static function target($first = 0) {} } class B extends A { function f() { parent::target(unavailable: 1); } }
