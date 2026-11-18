<?php
// Rule: class.notFound; class relative parameter.
namespace App;
function f(namespace\Missing $p) {} // error0
