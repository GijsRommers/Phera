<?php
// Rule: class.notFound; class good alias extends.
namespace App; use App\Known as AliasKnown;
class Known { public static function go() {} }
class C extends AliasKnown {}
