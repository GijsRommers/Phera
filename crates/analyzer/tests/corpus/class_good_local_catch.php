<?php
// Rule: class.notFound; class good local catch.
namespace App;
class Known { public static function go() {} }
try {} catch (Known $e) {}
