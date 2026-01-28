<?php
/**
 * @param string &...$s
 * @psalm-suppress UnusedParam
 */
function foo(&...$s) : void {}

$a = 1;
foo($a);
