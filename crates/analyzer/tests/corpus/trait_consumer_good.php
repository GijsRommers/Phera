<?php
// Rule: classConstant.notFound; trait_consumer_good.
trait T { function run() { echo self::LIMIT; } }
class Consumer { use T; const LIMIT = 7; }
