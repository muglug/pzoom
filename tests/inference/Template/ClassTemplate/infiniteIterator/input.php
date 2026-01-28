<?php

$input = range("a", "z");

$arrayIterator = new ArrayIterator($input);
$decoratorIterator = new InfiniteIterator($arrayIterator);
$key = $decoratorIterator->key();
$value = $decoratorIterator->current();
                