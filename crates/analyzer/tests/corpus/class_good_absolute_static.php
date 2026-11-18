<?php
// Rule: class.notFound; class good absolute static.
class Known { public static function go() {} }
\Known::go();
