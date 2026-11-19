<?php
// Rule: classConstant.notFound; trait_consumer_bad.
trait T { function run() { echo self::LIMIT; } }
class Consumer { use T; const OTHER_LIMIT = 7; }
