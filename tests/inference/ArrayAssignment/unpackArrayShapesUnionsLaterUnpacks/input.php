<?php
$shape = ["foo" => 1, "bar" => 2, 10 => 3];
/** @var array<int, 4> */
$a = [];
/** @var list<5> */
$b = [];
/** @var array<array-key, 6> */
$c = [];

$x = [...$a, ...$b, ...$c, ...$shape]; // Shape is last so it overrides previous
$y = [...$shape, ...$a, ...$b, ...$c]; // Shape is first, but only possibly matching keys union their values
                
