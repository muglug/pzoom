<?php
/**
 * @method static void bar()
 */
class A {}
class B extends A {}

/** @psalm-suppress UndefinedMethod, MixedAssignment */
$a = B::bar(123, "whatever");
