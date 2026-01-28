<?php
/**
 * @param string ...&$s
 * @psalm-suppress UnusedParam
 */
function foo(&...$s) : void {}

$b = 2;
foo($b);
