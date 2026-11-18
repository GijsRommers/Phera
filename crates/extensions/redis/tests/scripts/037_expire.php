<?php
require __DIR__."/../support.php";
foreach ([null,"NX","XX","GT","LT"] as $mode) {
$r->del($p."k"); check(fn()=>$r->expire($p."k",60000,$mode));
$r->set($p."k","v");check(fn()=>$r->expire($p."k",60000,$mode));
check(fn()=>$r->expire($p."k",60000,$mode));}
finish();
