<?php
// Rule: class.notFound; class good alias return.
namespace App; use App\Known as AliasKnown;
class Known { public static function go() {} }
function f(): ?AliasKnown { return null; }
