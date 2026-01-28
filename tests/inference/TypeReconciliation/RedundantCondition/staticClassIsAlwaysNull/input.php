<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    /**
     * @return ?static
     */
    public static function load() {
        return rand(0, 1)
            ? null
            : new static();
    }
}

$a = A::load();

if ($a && $a instanceof A) {}
