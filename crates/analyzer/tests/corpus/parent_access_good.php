<?php
// Rule: class.noParent; a parent call resolves the imported base.
class Base { public static function run() {} }
class Child extends Base { function f() { parent::run(); } }
