<?php
// Rule: class.notFound; class good local new.
namespace App;
class Known { public static function go() {} }
new Known;
