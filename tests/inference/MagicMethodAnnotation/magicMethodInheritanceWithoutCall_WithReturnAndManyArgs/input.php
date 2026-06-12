<?php
/**
 * @method void bar()
 */
class A {}
class B extends A {}

$obj = new B();

/** @psalm-suppress UndefinedMethod, MixedAssignment */
$a = $obj->bar(123, "whatever");
