<?php
/**
 * @param string[] &$s
 * @psalm-suppress UnusedParam
 */
function foo(&...$s) : void {}

$c = 3;
foo($c);
