<?php
// Rule: class.notFound; class good relative constant.
namespace App;
class Known { public static function go() {} }
namespace\Known::class;
