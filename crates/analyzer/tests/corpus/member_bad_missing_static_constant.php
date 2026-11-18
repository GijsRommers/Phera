<?php
// Rule: constant.notFound; member bad missing static constant.
class C { function f() { echo static::MISSING; } } // error0
