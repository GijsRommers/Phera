<?php
// Rule: method.notFound; member bad missing static method.
class C { function f() { static::missing(); } } // error0
