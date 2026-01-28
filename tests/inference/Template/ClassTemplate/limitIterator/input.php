<?php

$input = range("a", "z");

$arrayIterator = new ArrayIterator($input);
$decoratorIterator = new LimitIterator($arrayIterator, 1, 1);
$key = $decoratorIterator->key();
$value = $decoratorIterator->current();
                