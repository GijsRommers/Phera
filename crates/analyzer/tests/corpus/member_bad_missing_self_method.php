<?php
// Rule: method.notFound; member bad missing self method.
class C { function f() { self::missing(); } } // error0
