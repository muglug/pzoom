<?php
/**
 * @psalm-consistent-constructor
 */
abstract class A {
    /** @return static */
    public static function load() {
        return new static();
    }
}

class B extends A {
}

$b = B::load();
