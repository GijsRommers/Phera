<?php
// Rule: method.notFound; deliberate single error: method_self_static.
class C { static function f() { self::unavailable(); } }
