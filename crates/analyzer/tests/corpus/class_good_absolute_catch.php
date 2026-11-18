<?php
// Rule: class.notFound; class good absolute catch.
class Known { public static function go() {} }
try {} catch (\Known $e) {}
