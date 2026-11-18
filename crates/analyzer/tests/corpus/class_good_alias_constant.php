<?php
// Rule: class.notFound; class good alias constant.
namespace App; use App\Known as AliasKnown;
class Known { public static function go() {} }
AliasKnown::class;
