<?php
// Rule: class.notFound; class good relative new.
namespace App;
class Known { public static function go() {} }
new namespace\Known;
