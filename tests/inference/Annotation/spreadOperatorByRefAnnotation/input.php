<?php
/**
 * @param string &...$s
 * @psalm-suppress UnusedParam
 */
function foo(&...$s) : void {}
/**
 * @param string ...&$s
 * @psalm-suppress UnusedParam
 */
function bar(&...$s) : void {}
/**
 * @param string[] &$s
 * @psalm-suppress UnusedParam
 */
function bat(&...$s) : void {}

$a = "hello";
$b = "goodbye";
$c = "hello again";
foo($a);
bar($b);
bat($c);
