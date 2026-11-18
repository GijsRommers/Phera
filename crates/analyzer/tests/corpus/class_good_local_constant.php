<?php
// Rule: class.notFound; class good local constant.
namespace App;
class Known { public static function go() {} }
Known::class;
