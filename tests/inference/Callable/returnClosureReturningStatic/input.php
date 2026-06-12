<?php
/**
 * @psalm-consistent-constructor
 */
class C {
    /**
     * @return Closure():static
     */
    public static function foo() {
        return function() {
            return new static();
        };
    }
}
