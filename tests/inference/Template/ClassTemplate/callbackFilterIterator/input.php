<?php

$input = range("a", "z");

$arrayIterator = new ArrayIterator($input);
$decoratorIterator = new CallbackFilterIterator(
    $arrayIterator,
    static function (string $value): bool {return "a" === $value;}
);
$key = $decoratorIterator->key();
$value = $decoratorIterator->current();
                