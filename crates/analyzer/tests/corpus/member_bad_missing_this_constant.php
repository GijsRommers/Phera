<?php
// Rule: constant.notFound; member bad missing this constant.
class C { function f() { echo self::MISSING; } } // error0
