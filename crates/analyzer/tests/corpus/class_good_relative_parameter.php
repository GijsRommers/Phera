<?php
// Rule: class.notFound; class good relative parameter.
namespace App;
class Known { public static function go() {} }
function f(namespace\Known $p) {}
