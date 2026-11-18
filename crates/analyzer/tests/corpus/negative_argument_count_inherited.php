<?php
// Rule: arguments.count; deliberate single error: argument_count_inherited.
class A { static function f($first) {} } class B extends A {} B::f();
