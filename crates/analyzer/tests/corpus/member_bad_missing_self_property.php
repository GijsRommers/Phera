<?php
// Rule: property.notFound; member bad missing self property.
class C { function f() { echo self::$missing; } } // error0
