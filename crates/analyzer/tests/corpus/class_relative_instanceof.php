<?php
// Rule: class.notFound; class relative instanceof.
namespace App;
$x = null; $x instanceof namespace\Missing; // error0
