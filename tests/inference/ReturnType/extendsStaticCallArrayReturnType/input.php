<?php
/**
 * @psalm-consistent-constructor
 */
abstract class A {
    /** @return array<int,static> */
    public static function loadMultiple() {
        return [new static()];
    }
}

class B extends A {
}

$bees = B::loadMultiple();
