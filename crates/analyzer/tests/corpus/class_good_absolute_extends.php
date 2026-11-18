<?php
// Rule: class.notFound; class good absolute extends.
class Known { public static function go() {} }
class C extends \Known {}
