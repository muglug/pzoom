<?php

$input = range("a", "z");

$arrayIterator = new ArrayIterator($input);
$decoratorIterator = new CachingIterator($arrayIterator);
$next = $decoratorIterator->hasNext();
$key = $decoratorIterator->key();
$value = $decoratorIterator->current();
                