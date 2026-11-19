<?php
// Rule: staticProperty.notFound; static_closure_scope_bad.
class Receiver { public static $value = 1; }
class Binder {
    function run() { Closure::bind(static fn () => self::$missing, null, Receiver::class); }
}
