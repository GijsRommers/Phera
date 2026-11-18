<?php
// Rule: class.notFound; class alias instanceof.
namespace App; use Missing as AliasMissing;
$x = null; $x instanceof AliasMissing; // error0
