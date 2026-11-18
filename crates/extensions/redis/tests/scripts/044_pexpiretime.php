<?php
require __DIR__."/../support.php";
check(fn()=>$r->pexpiretime($p."k"));$r->set($p."k","v");
check(fn()=>$r->pexpiretime($p."k"));$r->expireAt($p."k",4102444800);
check(fn()=>$r->pexpiretime($p."missing"));
finish();
