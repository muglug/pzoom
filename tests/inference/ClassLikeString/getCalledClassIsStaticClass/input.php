<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    /** @return static */
    public function getStatic() {
        $c = get_called_class();
        return new $c();
    }
}
