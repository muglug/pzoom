<?php
/**
 * @method $this active()
 */
class A {
    public function __call(string $name, array $arguments) {}
}

/**
 * @mixin A
 */
class B {
    public function __call(string $name, array $arguments) {}
}

$b = new B;
$c = $b->active();
