<?php
// Rule: class.notFound; class good absolute return.
class Known { public static function go() {} }
function f(): ?\Known { return null; }
