<?php
// Rule: scope / names / signatures; construct good named function type.
function f(): DateTime { return new DateTime; } f()->format("Y");
