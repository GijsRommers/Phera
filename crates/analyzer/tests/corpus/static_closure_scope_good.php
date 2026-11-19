<?php
// Rule: staticProperty.notFound; static_closure_scope_good.
class Receiver { public static $value = 1; }
class Binder {
    function run() { Closure::bind(static fn () => self::$value, null, Receiver::class); }
}
