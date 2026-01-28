<?php
class A {}
class AChild extends A {}

/**
 * @param class-string<A> $s
 */
function foo(string $s) : void {}

foo(AChild::class);
