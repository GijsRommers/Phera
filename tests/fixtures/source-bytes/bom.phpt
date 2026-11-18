--TEST--
UTF-8 BOM is inline HTML and does not shift PHP line numbers
--FILE--
﻿<?php echo __LINE__, "\n";
--EXPECT--
﻿1
