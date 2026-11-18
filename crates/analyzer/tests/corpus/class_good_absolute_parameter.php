<?php
// Rule: class.notFound; class good absolute parameter.
class Known { public static function go() {} }
function f(\Known $p) {}
