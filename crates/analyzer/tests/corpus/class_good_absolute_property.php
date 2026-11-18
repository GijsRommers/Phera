<?php
// Rule: class.notFound; class good absolute property.
class Known { public static function go() {} }
class C { public \Known $p; }
