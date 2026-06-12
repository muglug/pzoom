<?php
/** @psalm-assert object{foo:string,bar:int} $value */
function assertObjectShape(mixed $value): void
{}

/** @var mixed $value */
$value = null;
assertObjectShape($value);
