<?php
$a = ["hello", 5];
/** @psalm-suppress RedundantFunctionCall */
$a_values = array_values($a);
$a_keys = array_keys($a);
