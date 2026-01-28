<?php
/**
 * @var callable $callable
 * @var array<string, int> $array
 */
$a = array_map($callable, $array);

/**
 * @var callable $callable
 * @var array<string, int> $array
 */
$b = array_map($callable, $array, $array);

/**
 * @var callable $callable
 * @var list<string> $list
 */
$c = array_map($callable, $list);

/**
 * @var callable $callable
 * @var list<string> $list
 */
$d = array_map($callable, $list, $list);
