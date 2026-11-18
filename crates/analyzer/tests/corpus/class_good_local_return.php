<?php
// Rule: class.notFound; class good local return.
namespace App;
class Known { public static function go() {} }
function f(): ?Known { return null; }
