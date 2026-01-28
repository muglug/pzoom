<?php
function takesArray(array $arr) : void {}

/** @param list<int> $arr */
function takesList(array $arr) : void {}

$a = [1, 2];

takesArray($a);
takesList($a);

$a[] = 3;

takesArray($a);
takesList($a);

$b = $a;

$b[] = rand(0, 10);
