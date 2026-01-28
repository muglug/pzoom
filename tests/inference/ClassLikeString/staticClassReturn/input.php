<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    /** @return static */
    public static function getInstance() {
        $class = static::class;
        return new $class();
    }
}
