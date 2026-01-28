<?php

$input = range("a", "z");

$arrayIterator = new ArrayIterator($input);
$decoratorIterator = new NoRewindIterator($arrayIterator);
$key = $decoratorIterator->key();
$value = $decoratorIterator->current();
                