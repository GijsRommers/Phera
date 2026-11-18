<?php
// Rule: class.notFound; class good local static.
namespace App;
class Known { public static function go() {} }
Known::go();
