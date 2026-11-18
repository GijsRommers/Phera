<?php
// Rule: class.notFound; class good alias catch.
namespace App; use App\Known as AliasKnown;
class Known { public static function go() {} }
try {} catch (AliasKnown $e) {}
