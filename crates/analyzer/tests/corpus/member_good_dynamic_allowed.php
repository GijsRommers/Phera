<?php
// Rule: members / constants / object contexts; member good dynamic allowed.
#[AllowDynamicProperties] class C {} $c = new C; $c->foo = 1;
