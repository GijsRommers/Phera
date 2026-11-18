<?php
// Rule: method.staticCall; deliberate single error: nonstatic_call_static_method.
class C { function f() {} static function g() { self::f(); } }
