<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    /** @return static */
    public static function getInstance() {
        return new static();
    }
}

final class AChild extends A {
    public static function getInstance() {
        return new AChild();
    }
}
