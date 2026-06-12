<?php
/**
 * @method static string foo()
 */
interface I {}

/**
 * @method static int bar()
 */
class A implements I {}

class B extends A {
    public static function __callStatic(string $name, array $arguments) {}
}

function consumeString(string $s): void {}
function consumeInt(int $i): void {}

consumeString(B::foo());
consumeInt(B::bar());
