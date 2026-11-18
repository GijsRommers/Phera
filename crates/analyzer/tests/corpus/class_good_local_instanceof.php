<?php
// Rule: class.notFound; class good local instanceof.
namespace App;
class Known { public static function go() {} }
$x = null; $x instanceof Known;
