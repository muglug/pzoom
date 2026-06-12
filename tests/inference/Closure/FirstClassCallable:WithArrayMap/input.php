<?php
$array = [1, 2, 3];
$closure = fn (int $value): int => $value * $value;
$result1 = array_map((new \SplQueue())->enqueue(...), $array);
$result2 = array_map(strval(...), $array);
$result3 = array_map($closure(...), $array);
