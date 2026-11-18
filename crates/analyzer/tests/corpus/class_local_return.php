<?php
// Rule: class.notFound; class local return.
namespace App;
function f(): ?Missing { return null; } // error0
