<?php
class MixinA {
    function a(): string { return "foo"; }
}

class MixinB {
    function b(): int { return 0; }
}

/**
 * @mixin MixinA
 * @mixin MixinB
 */
class Test {}

$test = new Test();

$a = $test->a();
$b = $test->b();
