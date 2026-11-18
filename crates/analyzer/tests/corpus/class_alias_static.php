<?php
// Rule: class.notFound; class alias static.
namespace App; use Missing as AliasMissing;
AliasMissing::go(); // error0
