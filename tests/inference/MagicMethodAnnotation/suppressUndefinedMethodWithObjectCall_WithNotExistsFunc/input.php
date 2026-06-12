<?php
/** @method int bar() */
class A {}
class B extends A {}

$obj = new B();

/** @psalm-suppress UndefinedMethod */
$a = $obj->bar(function_does_not_exist(123));
