<?php
// Rule: function.notFound; function bad import ns.
namespace App; use Missing as M; M\go(); // error0
