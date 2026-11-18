<?php
// Rule: members / constants / object contexts; member good enum backed.
enum E: string { case A = "a"; } echo E::from("a")->value;
