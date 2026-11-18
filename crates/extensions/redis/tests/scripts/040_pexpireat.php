<?php
require __DIR__."/../support.php";
foreach ([null,"NX","XX","GT","LT"] as $mode) {
$r->del($p."k"); check(fn()=>$r->pexpireAt($p."k",4102444800000,$mode));
$r->set($p."k","v");check(fn()=>$r->pexpireAt($p."k",4102444800000,$mode));
check(fn()=>$r->pexpireAt($p."k",4102444800000,$mode));}
finish();
