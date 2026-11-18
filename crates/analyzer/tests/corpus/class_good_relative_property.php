<?php
// Rule: class.notFound; class good relative property.
namespace App;
class Known { public static function go() {} }
class C { public namespace\Known $p; }
