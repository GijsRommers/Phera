<?php
require __DIR__."/../support.php";
foreach ([null,"NX","XX","GT","LT"] as $mode) {
$r->del($p."k"); check(fn()=>$r->expireAt($p."k",4102444800,$mode));
$r->set($p."k","v");check(fn()=>$r->expireAt($p."k",4102444800,$mode));
check(fn()=>$r->expireAt($p."k",4102444800,$mode));}
finish();
