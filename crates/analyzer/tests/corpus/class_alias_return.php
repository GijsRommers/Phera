<?php
// Rule: class.notFound; class alias return.
namespace App; use Missing as AliasMissing;
function f(): ?AliasMissing { return null; } // error0
