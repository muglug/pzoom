<?php
/** @method static int bar() */
class A {}
class B extends A {}

/** @psalm-suppress UndefinedMethod */
$a = B::bar(function_does_not_exist(123));
