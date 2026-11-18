<?php
// Rule: members / constants / object contexts; member good constant fallback.
namespace { const VALUE = 1; } namespace A { echo VALUE; }
