<?php
// Rule: class.notFound; class relative return.
namespace App;
function f(): ?namespace\Missing { return null; } // error0
