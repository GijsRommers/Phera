<?php
// Rule: class.notFound; class alias new.
namespace App; use Missing as AliasMissing;
new AliasMissing; // error0
