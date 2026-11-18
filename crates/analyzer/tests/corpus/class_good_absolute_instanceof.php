<?php
// Rule: class.notFound; class good absolute instanceof.
class Known { public static function go() {} }
$x = null; $x instanceof \Known;
