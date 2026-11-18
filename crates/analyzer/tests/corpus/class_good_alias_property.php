<?php
// Rule: class.notFound; class good alias property.
namespace App; use App\Known as AliasKnown;
class Known { public static function go() {} }
class C { public AliasKnown $p; }
