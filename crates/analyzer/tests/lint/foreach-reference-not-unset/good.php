<?php
foreach ($items as &$item) { $item++; } unset($item); $item = 0;
