<?php
/** @param list<int> $a */
function takesList($a): void {}

$a = [1, 1 => 2, 3];
takesList($a);
