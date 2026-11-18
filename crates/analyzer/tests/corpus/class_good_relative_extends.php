<?php
// Rule: class.notFound; class good relative extends.
namespace App;
class Known { public static function go() {} }
class C extends namespace\Known {}
