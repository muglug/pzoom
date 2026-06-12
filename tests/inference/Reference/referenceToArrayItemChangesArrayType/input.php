<?php
/** @var list<int> */
$arr = [];

assert(isset($arr[0]));
$int = &$arr[0];
$int = (string) $int;
