<?php
$arg = "is_string";

/**
 * @var array<string|int, float> $bar
 */
$keys = array_keys($bar);
$strings = array_filter($keys, $arg);
