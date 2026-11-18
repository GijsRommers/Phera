<?php
// Rule: class.notFound; class good local property.
namespace App;
class Known { public static function go() {} }
class C { public Known $p; }
