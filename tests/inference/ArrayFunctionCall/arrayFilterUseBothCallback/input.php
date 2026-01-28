<?php
/**
 * @var list<int> $arg
 */
$a = array_filter($arg, function (int $v, int $k) { return ($v > $k);}, ARRAY_FILTER_USE_BOTH);
