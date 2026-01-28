<?php
class A {}
class AChild extends A {}

/**
 * @param A::class $s
 */
function foo(string $s) : void {}

foo(AChild::class);
