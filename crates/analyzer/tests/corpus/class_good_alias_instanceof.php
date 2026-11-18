<?php
// Rule: class.notFound; class good alias instanceof.
namespace App; use App\Known as AliasKnown;
class Known { public static function go() {} }
$x = null; $x instanceof AliasKnown;
