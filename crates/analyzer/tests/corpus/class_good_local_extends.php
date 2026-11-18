<?php
// Rule: class.notFound; class good local extends.
namespace App;
class Known { public static function go() {} }
class C extends Known {}
