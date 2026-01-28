<?php
/** @var list<int> */
$a = [];
/** @var non-empty-list<string> */
$b = [];

$c = array_merge($a, $b);
$d = array_merge($b, $a);
