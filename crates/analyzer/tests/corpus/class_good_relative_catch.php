<?php
// Rule: class.notFound; class good relative catch.
namespace App;
class Known { public static function go() {} }
try {} catch (namespace\Known $e) {}
