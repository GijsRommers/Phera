<?php
// Rule: class.notFound; class alias parameter.
namespace App; use Missing as AliasMissing;
function f(AliasMissing $p) {} // error0
