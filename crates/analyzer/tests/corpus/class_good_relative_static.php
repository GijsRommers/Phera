<?php
// Rule: class.notFound; class good relative static.
namespace App;
class Known { public static function go() {} }
namespace\Known::go();
