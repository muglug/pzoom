<?php
class A {
    /** @var ?string */
    public $foo;
}

if (rand(0, 10) > 5) {
} elseif (($a = rand(0, 1) ? new A : null) && is_string($a->foo)) {}
