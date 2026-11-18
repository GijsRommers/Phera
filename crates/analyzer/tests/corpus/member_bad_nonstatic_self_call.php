<?php
// Rule: method.staticCall; member bad nonstatic self call.
class C { function go() {} static function f() { self::go(); } } // error0
