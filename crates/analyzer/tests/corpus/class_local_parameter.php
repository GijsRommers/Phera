<?php
// Rule: class.notFound; class local parameter.
namespace App;
function f(Missing $p) {} // error0
