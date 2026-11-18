<?php
// Rule: class.notFound; class good relative instanceof.
namespace App;
class Known { public static function go() {} }
$x = null; $x instanceof namespace\Known;
