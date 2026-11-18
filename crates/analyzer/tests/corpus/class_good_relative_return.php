<?php
// Rule: class.notFound; class good relative return.
namespace App;
class Known { public static function go() {} }
function f(): ?namespace\Known { return null; }
