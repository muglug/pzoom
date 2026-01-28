<?php
/** @param list<string> $arr */
function takesArray(array $arr) : void {}

$a = [];
$a[] = 1;
$a[] = 2;

takesArray($a);
