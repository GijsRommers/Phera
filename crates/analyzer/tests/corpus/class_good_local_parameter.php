<?php
// Rule: class.notFound; class good local parameter.
namespace App;
class Known { public static function go() {} }
function f(Known $p) {}
