<?php
require __DIR__."/../support.php";
check(fn()=>$r->pttl($p."k"));$r->set($p."k","v");
check(fn()=>$r->pttl($p."k"));$r->expireAt($p."k",4102444800);
check(fn()=>$r->pttl($p."missing"));
finish();
